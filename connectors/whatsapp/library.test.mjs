import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initAuthCreds, proto } from '@whiskeysockets/baileys';
import { spawnSync } from 'node:child_process';
import { createAuthState, Vault, replacer, reviver, mapMessage } from './core.mjs';

test('runner loads real dependencies and rejects missing configuration without authorization', () => {
  const result = spawnSync(process.execPath, ['index.mjs', 'run'], {
    cwd: new URL('.', import.meta.url), encoding: 'utf8', env: { PATH: process.env.PATH }
  });
  assert.equal(result.status, 1);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /Set TANGLED_SECRET_FILE/);
  assert.doesNotMatch(result.stderr, /at file:|privateKey|stack/);
});

test('real Baileys signal credentials and app-state keys survive encrypted restart', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'tangled-wa-library-'));
  try {
    const vault = new Vault(directory, Buffer.alloc(32, 7));
    let { state, saveCreds } = createAuthState(vault, { initAuthCreds, proto });
    const privateKey = state.creds.noiseKey.private;
    state.creds.registered = true;
    saveCreds();
    await state.keys.set({ session: { foreign: Buffer.from('signal-key') },
      'app-state-sync-key': { sync: { keyData: Buffer.from('app-state-key') } } });
    ({ state } = createAuthState(vault, { initAuthCreds, proto }));
    assert.deepEqual(state.creds.noiseKey.private, privateKey);
    assert.equal(state.creds.registered, true);
    assert.deepEqual((await state.keys.get('session', ['foreign'])).foreign, Buffer.from('signal-key'));
    const sync = (await state.keys.get('app-state-sync-key', ['sync'])).sync;
    assert.ok(sync instanceof proto.Message.AppStateSyncKeyData);
    assert.deepEqual(sync.keyData, Buffer.from('app-state-key'));
    await state.keys.set({ session: { foreign: null } });
    ({ state } = createAuthState(vault, { initAuthCreds, proto }));
    assert.deepEqual(await state.keys.get('session', ['foreign']), {});
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('actual protobuf history message retains bytes and source timestamp through spool serialization', () => {
  const message = proto.WebMessageInfo.fromObject({ key: { id: 'NATIVE', remoteJid: '123@s.whatsapp.net' },
    messageTimestamp: 1700000000, message: { imageMessage: { caption: 'history', mediaKey: Buffer.from('media-key') } } });
  const spooled = JSON.parse(JSON.stringify(message, replacer), reviver);
  const batch = mapMessage('owner@s.whatsapp.net', spooled, Buffer.from('attachment'));
  assert.equal(batch.objects[0].timestamp, '2023-11-14T22:13:20.000Z');
  assert.equal(batch.objects[0].text, 'history');
  assert.equal(batch.files[0].data_base64, Buffer.from('attachment').toString('base64'));
  assert.equal(batch.objects[0].data.message.imageMessage.mediaKey, Buffer.from('media-key').toString('base64'));
});
