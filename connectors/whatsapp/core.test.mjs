import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, statSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { withFetchTimeout, Vault, mapMessage, Destination, drainPending } from './core.mjs';
import { spawnSync } from 'node:child_process';

test('source fetch gets an actual deadline even when its caller omits timeout options', async () => {
  const keepAlive = setTimeout(() => {}, 1000);
  try {
    const sourceFetch = async (url, options) => {
      assert.equal(options.headers.Source, 'test');
      return new Promise((_, reject) => options.signal.addEventListener('abort', () => reject(options.signal.reason), { once: true }));
    };
    await assert.rejects(withFetchTimeout(sourceFetch, 10)('https://source.invalid', { headers: { Source: 'test' } }), { name: 'TimeoutError' });
  } finally { clearTimeout(keepAlive); }
});

test('dependency console output cannot expose signal sessions', () => {
  const result = spawnSync(process.execPath, ['--input-type=module', '-e',
    "import './quiet-console.mjs'; for(const method of ['log','info','warn','error','debug']) console[method]({privateKey:'must-stay-private'}); process.stdout.write('safe');"],
    { cwd: new URL('.', import.meta.url), encoding: 'utf8' });
  assert.equal(result.status, 0);
  assert.equal(result.stdout, 'safe');
  assert.equal(result.stderr, '');
});

test('encrypted credentials roundtrip and wrong key fails', () => {
  const dir = mkdtempSync(join(tmpdir(), 'tangled-wa-'));
  try {
    const vault = new Vault(dir, Buffer.alloc(32, 1));
    vault.save('auth', { secret: 'credential', buffer: Buffer.from('keys') });
    assert.equal(readFileSync(join(dir, 'auth.enc')).includes('credential'), false);
    assert.equal(statSync(join(dir, 'auth.enc')).mode & 0o777, 0o600);
    assert.equal(vault.load('auth').secret, 'credential');
    assert.deepEqual(vault.load('auth').buffer, Buffer.from('keys'));
    assert.throws(() => new Vault(dir, Buffer.alloc(32, 2)).load('auth'));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('native message identity includes conversation, sender stays foreign and media stays bytes', () => {
  const message = { key: { remoteJid: 'group@g.us', participant: '12@s.whatsapp.net', id: 'A' },
    messageTimestamp: 1700000000, message: { imageMessage: { caption: 'hi', mimetype: 'image/jpeg' } }, unknown: 'keep' };
  const batch = mapMessage('owner@s.whatsapp.net', message, Buffer.from('image'));
  assert.equal(batch.objects[0].id, 'group@g.us:A');
  assert.equal(batch.objects[0].sender, '12@s.whatsapp.net');
  assert.equal(batch.objects[0].timestamp, '2023-11-14T22:13:20.000Z');
  assert.equal(batch.objects[0].data.unknown, 'keep');
  assert.equal(Buffer.from(batch.files[0].data_base64, 'base64').toString(), 'image');
});

test('failed destination retains encrypted pending message; success removes it', async () => {
  const pending = { a: { source: 'whatsapp', objects: [] } };
  let saves = 0;
  await assert.rejects(drainPending(pending, { send: async () => { throw Error('offline'); } }, () => saves++));
  assert.ok(pending.a);
  assert.equal(saves, 0);
  await drainPending(pending, { send: async () => {} }, () => saves++);
  assert.deepEqual(pending, {});
  assert.equal(saves, 1);
});

test('destination retries bounded transient failures, never retries revoked token', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'tangled-wa-'));
  try {
    const vault = new Vault(dir, Buffer.alloc(32, 1));
    // fetch injection is the network boundary; the implementation still builds real requests.
    let calls = 0;
    const destination = new Destination('https://archive.example', 'a/b', 'private-token', {
      fetch: async (url, request) => {
        calls++;
        assert.equal(url, 'https://archive.example/api/imports/connections/a%2Fb/ingest');
        assert.equal(request.headers.Authorization, 'Bearer private-token');
        return { ok: false, status: 401 };
      }, sleep: async () => {}
    });
    await assert.rejects(destination.send({ source: 'whatsapp' }));
    assert.equal(calls, 1);
    destination.fetch = async () => { calls++; return { ok: false, status: 503 }; };
    calls = 0;
    await assert.rejects(destination.send({ source: 'whatsapp' }));
    assert.equal(calls, 5);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
