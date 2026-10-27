import { createCipheriv, createDecipheriv, createHash, randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, lstatSync, readFileSync, writeFileSync, openSync, closeSync,
  fsyncSync, renameSync, unlinkSync, chmodSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

export class ConnectorError extends Error {}

export function withFetchTimeout(sourceFetch, milliseconds = 60000) {
  return (url, options = {}) => sourceFetch(url, { ...options,
    signal: options.signal
      ? AbortSignal.any([options.signal, AbortSignal.timeout(milliseconds)])
      : AbortSignal.timeout(milliseconds) });
}

export const MAX_MEDIA = Number(process.env.TANGLED_MAX_MEDIA_BYTES || 8 * 1024 * 1024);
export const replacer = (_, value) => {
  if (value instanceof Uint8Array) return { type: 'Buffer', data: [...value] };
  if (typeof value === 'bigint') return value.toString();
  return value;
};
export const reviver = (_, value) => value?.type === 'Buffer' && Array.isArray(value.data)
  ? Buffer.from(value.data) : value;

export function privateFile(path) {
  const info = lstatSync(path);
  if (!info.isFile() || info.isSymbolicLink() || (info.mode & 0o077)) {
    throw new ConnectorError('Secret file must be a regular non-symlink file with mode 0600');
  }
  return readFileSync(path);
}

export class Vault {
  constructor(directory, secret) {
    if (secret.length < 32) throw new ConnectorError('Encryption key file must contain at least 32 random bytes');
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    const info = lstatSync(directory);
    if (info.isSymbolicLink() || (info.mode & 0o077)) throw new ConnectorError('State directory requires mode 0700');
    this.directory = directory;
    this.key = createHash('sha256').update(secret).digest();
  }

  path(name) {
    if (!/^[a-zA-Z0-9_-]+$/.test(name)) throw new ConnectorError('Invalid local state name');
    return join(this.directory, `${name}.enc`);
  }

  load(name, fallback = {}) {
    if (!existsSync(this.path(name))) return fallback;
    try {
      const blob = privateFile(this.path(name));
      if (blob.subarray(0, 4).toString() !== 'WCV1') throw new ConnectorError('version');
      const decipher = createDecipheriv('aes-256-gcm', this.key, blob.subarray(4, 16));
      decipher.setAAD(Buffer.from(name));
      decipher.setAuthTag(blob.subarray(16, 32));
      return JSON.parse(Buffer.concat([decipher.update(blob.subarray(32)), decipher.final()]).toString(), reviver);
    } catch { throw new ConnectorError('Cannot decrypt local state; verify key and state integrity'); }
  }

  save(name, value) {
    const nonce = randomBytes(12);
    const cipher = createCipheriv('aes-256-gcm', this.key, nonce);
    cipher.setAAD(Buffer.from(name));
    const ciphertext = Buffer.concat([cipher.update(JSON.stringify(value, replacer)), cipher.final()]);
    const blob = Buffer.concat([Buffer.from('WCV1'), nonce, cipher.getAuthTag(), ciphertext]);
    const temporary = join(this.directory, `.state-${randomBytes(12).toString('hex')}`);
    try {
      const fd = openSync(temporary, 'wx', 0o600);
      try { writeFileSync(fd, blob); fsyncSync(fd); } finally { closeSync(fd); }
      renameSync(temporary, this.path(name));
      const directoryFd = openSync(this.directory, 'r');
      try { fsyncSync(directoryFd); } finally { closeSync(directoryFd); }
    } finally { if (existsSync(temporary)) unlinkSync(temporary); }
  }

  lock() {
    const path = join(this.directory, 'runner.lock');
    try {
      const fd = openSync(path, 'wx', 0o600);
      writeFileSync(fd, String(process.pid));
      closeSync(fd);
    } catch {
      throw new ConnectorError('State directory is locked; stop the other runner, or remove runner.lock after verifying its process is gone');
    }
    return () => { try { unlinkSync(path); } catch {} };
  }
}

export function createAuthState(vault, { initAuthCreds, proto }) {
  const auth = vault.load('auth', null) || { creds: initAuthCreds(), keys: {} };
  const saveCreds = () => vault.save('auth', auth);
  saveCreds();
  return { saveCreds, state: { creds: auth.creds, keys: {
    get: async (type, ids) => {
      const found = {};
      for (const id of ids) {
        const value = auth.keys[type]?.[id];
        if (value) found[id] = type === 'app-state-sync-key'
          ? proto.Message.AppStateSyncKeyData.fromObject(value) : value;
      }
      return found;
    },
    set: async values => {
      for (const [type, entries] of Object.entries(values)) {
        auth.keys[type] ||= {};
        for (const [id, value] of Object.entries(entries)) {
          if (value == null) delete auth.keys[type][id];
          else auth.keys[type][id] = value;
        }
      }
      saveCreds();
    }
  } } };
}

export class Destination {
  constructor(base, connection, token, options = {}) {
    const url = new URL(base);
    if (url.username || url.password || url.search || url.hash || !['http:', 'https:'].includes(url.protocol)) {
      throw new ConnectorError('Destination must be a base HTTP(S) URL without credentials or query');
    }
    if (url.protocol !== 'https:' && !['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)) {
      throw new ConnectorError('Remote destinations require HTTPS');
    }
    this.url = base.replace(/\/+$/, '') + '/api/imports/connections/' + encodeURIComponent(connection) + '/ingest';
    this.token = token;
    this.fetch = options.fetch || fetch;
    this.sleep = options.sleep || sleep;
  }

  async send(batch) {
    const body = JSON.stringify(batch, replacer);
    if (Buffer.byteLength(body) > 128 * 1024 * 1024) throw new ConnectorError('Batch exceeds 128 MiB; lower media byte limit');
    const token = typeof this.token === 'function' ? this.token() : this.token;
    if (!token || /[\r\n]/.test(token)) throw new ConnectorError('Invalid ingest token');
    for (let attempt = 0; attempt < 5; attempt++) {
      let response;
      try {
        response = await this.fetch(this.url, { method: 'POST', redirect: 'error',
          headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
          body, signal: AbortSignal.timeout(60000) });
      } catch { /* Only network failures and timeouts retry. */ }
      if (response?.ok) { await response.body?.cancel(); return; }
      if (response && response.status !== 429 && response.status < 500) {
        await response.body?.cancel();
        throw new ConnectorError(`Destination rejected import (HTTP ${response.status}); pending data retained`);
      }
      await response?.body?.cancel();
      if (attempt < 4) await this.sleep(1000 * 2 ** attempt);
    }
    throw new ConnectorError('Destination unavailable after 5 attempts; pending data retained');
  }
}

export function content(message) {
  let value = message.message || {};
  for (let i = 0; i < 8; i++) {
    const nested = value.ephemeralMessage?.message || value.viewOnceMessage?.message ||
      value.viewOnceMessageV2?.message || value.documentWithCaptionMessage?.message ||
      value.editedMessage?.message;
    if (!nested) break;
    value = nested;
  }
  return value;
}

export function mediaContent(message) {
  const body = content(message);
  return body.imageMessage || body.videoMessage || body.audioMessage || body.documentMessage || body.stickerMessage;
}

export function mapMessage(account, raw, media, warnings = []) {
  const key = raw.key || {};
  if (!key.remoteJid || !key.id) throw new ConnectorError('WhatsApp message is missing native identity');
  const body = content(raw);
  const attachment = mediaContent(raw);
  const files = [];
  if (media != null) {
    files.push({ name: `whatsapp/${key.remoteJid}/${key.id}/media`,
      media_type: attachment?.mimetype || 'application/octet-stream', data_base64: media.toString('base64') });
  }
  const seconds = Number(raw.messageTimestamp);
  const timestamp = seconds && Number.isFinite(seconds) ? new Date(seconds * 1000).toISOString() : null;
  return { source: 'whatsapp', account, objects: [{ id: `${key.remoteJid}:${key.id}`,
    kind: 'message', conversation: key.remoteJid,
    sender: key.fromMe ? account : (key.participant || key.remoteJid), timestamp,
    text: body.conversation || body.extendedTextMessage?.text || attachment?.caption || null,
    attachments: files.map(f => f.name), data: raw }], files, warnings };
}

export async function drainPending(pending, destination, save) {
  for (const [id, batch] of Object.entries(pending)) {
    await destination.send(batch);
    delete pending[id];
    save();
  }
}
