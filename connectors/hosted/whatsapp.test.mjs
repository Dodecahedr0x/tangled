import test from 'node:test';
import assert from 'node:assert/strict';
import { qrSvg, hostedDestination } from './whatsapp.mjs';
import { Vault } from '../whatsapp/core.mjs';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('QR credential is rendered as a bounded SVG with no raw source text', () => {
  const svg = qrSvg('2@source-login-credential');
  assert.match(svg, /^<svg xmlns="http:\/\/www.w3.org\/2000\/svg"/);
  assert.match(svg, /viewBox="0 0 [0-9]+ [0-9]+"/);
  assert.match(svg, /fill="black"/);
  assert.ok(svg.length < 256 * 1024);
  assert.ok(!svg.includes('source-login-credential'));
});

test('worker uses encrypted connection binding instead of token file', async () => {
  const directory = mkdtempSync(join(tmpdir(), 'hosted-wa-'));
  try {
    const vault = new Vault(directory, Buffer.alloc(32, 107));
    vault.save('hosted-binding', { connection_id: 'connection-1', token: 'wa-ingest-secret' });
    let received;
    const destination = hostedDestination(vault, {
      TANGLED_URL: 'http://127.0.0.1:4000', TANGLED_CONNECTION_ID: 'connection-1'
    });
    destination.fetch = async (url, options) => {
      received = { url, authorization: options.headers.Authorization };
      return { ok: true };
    };
    await destination.send({ source: 'whatsapp', account: '123@s.whatsapp.net', objects: [] });
    assert.equal(received.authorization, 'Bearer wa-ingest-secret');
    assert.match(received.url, /connections\/connection-1\/ingest$/);
    assert.ok(!readFileSync(vault.path('hosted-binding')).includes('wa-ingest-secret'));
    assert.throws(() => hostedDestination(vault, {
      TANGLED_URL: 'http://127.0.0.1:4000', TANGLED_CONNECTION_ID: 'other-connection'
    }), /binding/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
