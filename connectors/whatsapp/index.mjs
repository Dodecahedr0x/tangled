import './quiet-console.mjs';
import { qrSvg, hostedDestination } from '../hosted/whatsapp.mjs';
import { Console } from 'node:console';
import makeWASocket, { initAuthCreds, proto, DisconnectReason, downloadMediaMessage,
  jidNormalizedUser, Browsers } from '@whiskeysockets/baileys';
import pino from 'pino';
import qrcode from 'qrcode-terminal';
import { createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { withFetchTimeout, ConnectorError, createAuthState, Vault, Destination, privateFile, replacer, reviver, mapMessage, mediaContent, MAX_MEDIA } from './core.mjs';

// No source logger output: library debug/error objects can include decrypted messages and auth state.
const logger = pino({ level: 'silent' });
const output = new Console({ stdout: process.stdout, stderr: process.stderr });
// Baileys 7rc14's getHttpStream drops RequestInit.signal. Wrap its global fetch
// boundary so stalled media requests still abort after a bounded deadline.
globalThis.fetch = withFetchTimeout(globalThis.fetch);
const mode = process.argv[2] || 'run';
let releaseLock;
let socket;
let stopping = false;

function fail(error) {
  if (error instanceof ConnectorError) output.error(error.message);
  output.error('WhatsApp connector stopped; encrypted pending data is retained. Check setup, source access and destination, then restart.');
  stopping = true;
  socket?.end(new Error('connector stopped'));
  releaseLock?.();
  process.exit(1);
}

async function main() {
  if (!['login', 'run'].includes(mode)) throw new ConnectorError('Usage: index.mjs [login|run]');
  if (!process.env.TANGLED_SECRET_FILE) throw new ConnectorError('Set TANGLED_SECRET_FILE');
  const directory = process.env.TANGLED_STATE_DIR || join(homedir(), '.local/share/tangled-connectors/whatsapp');
  const vault = new Vault(directory, privateFile(process.env.TANGLED_SECRET_FILE));
  releaseLock = vault.lock();
  const { state, saveCreds } = createAuthState(vault, { initAuthCreds, proto });
  const sourceState = vault.load('source');
  const pending = vault.load('pending');
  const persistPending = () => vault.save('pending', pending);
  let destination;
  if (mode === 'run') {
    if (!process.env.TANGLED_URL || !process.env.TANGLED_CONNECTION_ID ||
      (process.env.TANGLED_HOSTED !== '1' && !process.env.TANGLED_INGEST_TOKEN_FILE)) {
      throw new ConnectorError('Set TANGLED_URL, TANGLED_CONNECTION_ID, TANGLED_INGEST_TOKEN_FILE');
    }
    destination = process.env.TANGLED_HOSTED === '1' ? hostedDestination(vault)
      : new Destination(process.env.TANGLED_URL, process.env.TANGLED_CONNECTION_ID,
        () => privateFile(process.env.TANGLED_INGEST_TOKEN_FILE).toString().trim());
    const binding = createHash('sha256').update(destination.url).digest('hex');
    if (sourceState.destination && sourceState.destination !== binding) {
      throw new ConnectorError('State is bound to another connection; use a new state directory or explicitly migrate it');
    }
    sourceState.destination = binding;
    vault.save('source', sourceState);
  }
  let account;
  let draining = false;
  async function drain() {
    if (!destination || !account || draining) return;
    draining = true;
    try {
      while (Object.keys(pending).length) {
        const [id, entry] = Object.entries(pending)[0];
        if (entry.account && entry.account !== account) throw new ConnectorError('Pending account mismatch');
        let media;
        const warnings = [];
        if (entry.type === 'message') {
          const attachment = mediaContent(entry.raw);
          if (attachment && Number(attachment.fileLength) > MAX_MEDIA) {
            warnings.push('WhatsApp media exceeds configured byte limit; original source metadata retained');
          } else if (attachment) {
            // Stream and bound downloads; do not ask peers to re-upload expired media.
            let stream;
            for (let attempt = 0; attempt < 5; attempt++) {
              try {
                stream = await downloadMediaMessage(entry.raw, 'stream', {
                  options: { signal: AbortSignal.timeout(60000) }
                });
                const chunks = [];
                let length = 0;
                for await (const chunk of stream) {
                  length += chunk.length;
                  if (length > MAX_MEDIA) {
                    stream.destroy();
                    warnings.push('WhatsApp media exceeds configured byte limit; source metadata retained');
                    break;
                  }
                  chunks.push(chunk);
                }
                if (length <= MAX_MEDIA) media = Buffer.concat(chunks);
                break;
              } catch {
                stream?.destroy();
                if (attempt === 4) {
                  // Metadata delivery is explicit about expired/unavailable history media.
                  warnings.push('WhatsApp media unavailable after 5 download attempts; source metadata retained');
                } else await sleep(1000 * 2 ** attempt);
              }
            }
          }
          const batch = mapMessage(account, entry.raw, media, warnings);
          await destination.send(batch);
        } else {
          await destination.send({ source: 'whatsapp', account, objects: [{
            id: `event:${id}`, kind: entry.type, conversation: entry.raw.key?.remoteJid || null,
            sender: entry.raw.key?.participant || null, timestamp: null, text: null,
            attachments: [], data: entry.raw }], files: [], warnings: [] });
        }
        delete pending[id];
        persistPending();
      }
    } finally { draining = false; }
  }
  function enqueue(type, raw) {
    if (Object.keys(pending).length >= 10000) throw new ConnectorError('Pending queue reached 10000 messages; resume destination first');
    // Round-trip preserves source bytes while making protobuf/Long metadata JSON-compatible.
    const json = JSON.stringify(raw, replacer);
    const id = createHash('sha256').update(type + json).digest('hex');
    pending[id] = { type, raw: JSON.parse(json, reviver), account: account || null };
    if (Buffer.byteLength(JSON.stringify(pending, replacer)) > 128 * 1024 * 1024) {
      throw new ConnectorError('Pending queue reached 128 MiB; resume destination first');
    }
    persistPending();
  }
  process.on('SIGINT', () => {
    stopping = true;
    socket?.end(new Error('shutdown'));
    releaseLock?.();
    process.exit(0);
  });
  process.on('SIGTERM', () => {
    stopping = true;
    socket?.end(new Error('shutdown'));
    releaseLock?.();
    process.exit(0);
  });
  let failures = 0;
  while (!stopping && failures < 5) {
    let reason;
    await new Promise(resolve => {
      socket = makeWASocket({ auth: state, logger, browser: Browsers.macOS('Desktop'),
        syncFullHistory: true, markOnlineOnConnect: false, connectTimeoutMs: 60000,
        defaultQueryTimeoutMs: 60000, retryRequestDelayMs: 1000,
        shouldSyncHistoryMessage: () => true,
        getMessage: async () => undefined });
      socket.ev.on('creds.update', update => { Object.assign(state.creds, update); saveCreds(); });
      socket.ev.on('connection.update', update => {
        if (update.qr) {
          if (mode !== 'login') { reason = 'login'; socket.end(new Error('login required')); resolve(); return; }
          if (process.env.TANGLED_HOSTED_LOGIN === '1') {
            output.log(JSON.stringify({ qr_svg: qrSvg(update.qr) }));
          } else {
            if (!process.stdout.isTTY) { fail(); return; }
            output.log('Scan in WhatsApp → Linked devices → Link a device (QR is a login credential):');
            qrcode.generate(update.qr, { small: true }, text => output.log(text));
          }
        }
        if (update.connection === 'open') {
          account = jidNormalizedUser(socket.user.id);
          if ((sourceState.account && sourceState.account !== account) ||
              (process.env.TANGLED_ACCOUNT && process.env.TANGLED_ACCOUNT !== account)) { fail(); return; }
          sourceState.account = account;
          vault.save('source', sourceState);
          if (process.env.TANGLED_HOSTED_LOGIN === '1') output.log(JSON.stringify({ account }));
          else output.log(`WhatsApp source account: ${account}`);
          if (mode === 'login' && process.env.TANGLED_HOSTED_LOGIN !== '1') output.log('History is being saved locally. Leave running during sync; Ctrl-C when ready, then configure the destination and run.');
          drain().catch(fail);
        }
        if (update.connection === 'close') {
          reason = update.lastDisconnect?.error?.output?.statusCode;
          resolve();
        }
      });
      function capture(type, values) {
        try { for (const raw of values) enqueue(type, raw); }
        catch (error) { fail(error); return; }
        drain().catch(fail);
      }
      socket.ev.on('messages.upsert', ({ messages }) => capture('message', messages));
      socket.ev.on('messaging-history.set', history => {
        capture('message', history.messages || []);
        capture('history_metadata', [{ chats: history.chats, contacts: history.contacts,
          progress: history.progress, syncType: history.syncType, isLatest: history.isLatest,
          coverage: 'WhatsApp supplies linked-device history; complete lifetime history is not guaranteed.' }]);
      });
      socket.ev.on('messages.update', values => capture('message_update', values));
      socket.ev.on('messages.delete', values => capture('message_delete', [values]));
      socket.ev.on('messages.reaction', values => capture('reaction', values));
      socket.ev.on('contacts.upsert', values => capture('contact', values));
      socket.ev.on('chats.upsert', values => capture('conversation', values));
    });
    if (stopping) break;
    if (reason === DisconnectReason.loggedOut || reason === 'login') {
      throw new ConnectorError('WhatsApp authorization required; run index.mjs login');
    }
    failures++;
    if (failures < 5) await sleep(reason === DisconnectReason.restartRequired ? 1000 : 1000 * 2 ** failures);
  }
  if (!stopping) throw new ConnectorError('WhatsApp connection failed 5 times');
}

process.on('uncaughtException', fail);
process.on('unhandledRejection', fail);
main().catch(fail);
