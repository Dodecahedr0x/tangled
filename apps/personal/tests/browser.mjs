import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { generateKeyPairSync } from 'node:crypto';
import { chromium } from 'playwright';

const repo = fileURLToPath(new URL('../../../', import.meta.url));
const binary = process.env.TANGLED_PERSONAL_APP || path.join(repo, 'target/debug/tangled-personal');
const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'personal-browser-test-'));
const screenshots = process.env.TANGLED_UI_SCREENSHOTS;
const app = spawn(binary, ['serve', '--data', path.join(directory, 'library'), '--listen', '127.0.0.1:0', '--no-open'], { stdio: ['ignore', 'pipe', 'pipe'] });
let browser;
try {
  const launch = await new Promise((resolve, reject) => {
    let output = '', errors = '';
    const timeout = setTimeout(() => reject(new Error('Personal did not start within 20 seconds')), 20000);
    app.stderr.on('data', chunk => { errors += chunk; });
    app.once('error', reject);
    app.once('exit', code => reject(new Error(`Personal exited (${code}): ${errors}`)));
    app.stdout.on('data', chunk => {
      output += chunk;
      const line = output.split('\n').find(line => /^http:\/\/127\.0\.0\.1:\d+\/#session=[a-f0-9]+$/.test(line));
      if (line) { clearTimeout(timeout); resolve(line); }
    });
  });
  const url = new URL(launch), token = new URLSearchParams(url.hash.slice(1)).get('session');
  const api = async (feature, input) => {
    const response = await fetch(`${url.origin}/api/v1/personal/${feature}`, { method: 'POST', headers: { 'content-type': 'application/json', 'x-tangled-session': token }, body: JSON.stringify(input) });
    const value = await response.json(); assert.equal(response.ok, true, value.error); return value;
  };
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true });
  const page = await context.newPage(), errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(launch);
  await page.waitForFunction(() => document.querySelector('#library-status')?.textContent.includes('Connected'));
  const go = async label => { await page.getByRole('link', { name: label, exact: true }).click(); await page.getByRole('heading', { name: label, level: 1, exact: true }).waitFor(); await page.waitForLoadState('networkidle'); };
  const fill = (label, text) => page.getByLabel(label, { exact: true }).fill(text);
  const click = async name => { await page.getByRole('button', { name, exact: true }).click(); await page.waitForFunction(() => !document.querySelector('[aria-busy="true"]')); };
  const downloaded = async action => { const pending = page.waitForEvent('download'); await action(); const download = await pending; const stream = await download.createReadStream(); const chunks = []; for await (const chunk of stream) chunks.push(chunk); return { name: download.suggestedFilename(), bytes: Buffer.concat(chunks) }; };

  const isolated = await browser.newContext();
  const disconnected = await isolated.newPage();
  await disconnected.goto(url.origin);
  await disconnected.waitForFunction(() => typeof window.BroadcastChannel === 'function' && document.querySelector('#view'));
  await disconnected.goto(launch);
  await disconnected.waitForFunction(() => document.querySelector('#library-status')?.textContent.includes('Connected'));
  await disconnected.getByRole('link', { name: 'Notes', exact: true }).click();
  await disconnected.getByRole('heading', { name: 'Notes', level: 1, exact: true }).waitFor();
  await disconnected.getByLabel('Text', { exact: true }).fill('Keep this draft during reconnect');
  await disconnected.waitForTimeout(2200);
  assert.equal(await disconnected.getByLabel('Text', { exact: true }).inputValue(), 'Keep this draft during reconnect');
  assert.match(await disconnected.locator('#library-status').textContent(), /Connected/);
  assert.equal(await disconnected.evaluate(() => location.hash), '#notes');
  await disconnected.evaluate(() => sessionStorage.setItem('tangled-personal.session', '0'.repeat(64)));
  await disconnected.getByLabel('Text', { exact: true }).fill('');
  await disconnected.reload();
  await disconnected.getByRole('button', { name: 'Reconnect', exact: true }).waitFor();
  await disconnected.goto(launch);
  await disconnected.waitForFunction(() => document.querySelector('#library-status')?.textContent.includes('Connected'));
  await isolated.close();

  const peer = await context.newPage(); await peer.goto(url.origin);
  await peer.waitForFunction(() => document.querySelector('#library-status')?.textContent.includes('Connected'));
  await peer.close();
  await page.evaluate(() => { location.hash = 'constructor'; });
  await page.getByRole('heading', { name: 'Today', level: 1, exact: true }).waitFor();

  await go('Notes');
  await page.locator('.skip-link').evaluate(link => link.click());
  assert.equal(await page.evaluate(() => location.hash), '#notes');
  assert.equal(await page.evaluate(() => document.activeElement.id), 'workspace');
  await fill('Title (optional)', 'Weekend plans'); await fill('Text', 'Walk by the river, then make a quiet place to read.');
  await page.getByLabel('Keep as', { exact: true }).selectOption('draft');
  let failOnce = true;
  await page.route('**/api/v1/personal/notes', async route => {
    if (failOnce && route.request().postDataJSON().action === 'create') { failOnce = false; await route.fulfill({ status: 422, contentType: 'application/json', body: JSON.stringify({ error: 'Temporary test failure. Retry your note.' }) }); }
    else await route.continue();
  });
  await click('Create note'); await page.getByRole('alert').filter({ hasText: 'Temporary test failure' }).waitFor();
  assert.equal(await page.getByLabel('Text', { exact: true }).inputValue(), 'Walk by the river, then make a quiet place to read.');
  await click('Create note'); await page.getByRole('button', { name: 'Weekend plans', exact: true }).waitFor();
  await page.unroute('**/api/v1/personal/notes');
  await page.getByText('Attachments (0)', { exact: true }).click();
  await page.getByLabel('File to attach', { exact: true }).setInputFiles({ name: 'agenda.txt', mimeType: 'text/plain', buffer: Buffer.from('Bring the field journal.') });
  await click('Attach file'); await page.getByText('Attachments (1)', { exact: true }).click(); await page.getByText('agenda.txt', { exact: true }).waitFor();
  assert.equal((await downloaded(() => click('Download'))).bytes.toString(), 'Bring the field journal.');

  await go('Capture');
  assert.equal(await page.getByLabel('Saved text', { exact: true }).isVisible(), false);
  await page.getByLabel('What are you saving?', { exact: true }).selectOption('article');
  await fill('Title (optional)', 'Slow reading'); await fill('Original URL', 'https://example.test/reading');
  await fill('Saved text', 'Orchid fieldnotes for a quiet weekend. Saved here to read offline.'); await fill('Tags', 'weekend, reading');
  await click('Save item'); await page.getByRole('button', { name: 'Slow reading', exact: true }).waitFor();
  await click('Slow reading'); await page.getByRole('button', { name: 'Star item', exact: true }).waitFor();
  await click('Star item'); await page.getByRole('button', { name: 'Remove star', exact: true }).waitFor();
  assert.match((await downloaded(() => click('Download original'))).bytes.toString(), /Orchid fieldnotes/);
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a/RcAAAAASUVORK5CYII=', 'base64');
  await page.getByLabel('What are you saving?', { exact: true }).selectOption('screenshot');
  await fill('Title (optional)', 'Field image');
  await page.getByLabel('Choose a file', { exact: true }).setInputFiles({ name: 'field.png', mimeType: 'image/png', buffer: png });
  await click('Save item'); await page.getByRole('button', { name: 'Field image', exact: true }).waitFor();

  await go('Library'); await fill('Search your library', 'Orchid'); await click('Search');
  await page.getByRole('button', { name: 'Slow reading', exact: true }).waitFor(); await click('Slow reading');
  await page.getByText('Orchid fieldnotes for a quiet weekend. Saved here to read offline.', { exact: true }).waitFor();

  await go('Reading'); await page.getByText('Subscriptions and feed imports', { exact: true }).click();
  await fill('Feed URL', 'https://example.test/journal.xml'); await fill('Subscription title', 'Field journal'); await click('Subscribe');
  await page.getByLabel('Import into subscription', { exact: true }).selectOption('https://example.test/journal.xml');
  const rss = '<rss version="2.0"><channel><item><guid>garden</guid><title>The quiet garden</title><description><![CDATA[<p>Morning light across the <b>orchids</b>.</p>]]></description></item></channel></rss>';
  await page.getByLabel('RSS or Atom file', { exact: true }).setInputFiles({ name: 'journal.xml', mimeType: 'application/xml', buffer: Buffer.from(rss) });
  await click('Import feed file'); await page.getByRole('button', { name: 'The quiet garden', exact: true }).waitFor(); await click('The quiet garden');
  await page.getByRole('button', { name: /Star/ }).last().waitFor();
  const readerText = await page.getByRole('article', { name: 'Feed reader' }).innerText();
  assert.match(readerText, /Morning light across the orchids/); assert.doesNotMatch(readerText, /<p>|<b>/);

  await go('Connections'); await page.getByText('Import recovery activity', { exact: true }).click();
  const activity = [{ source: 'telegram', account: 'field-owner', object: { id: '42', kind: 'message', conversation: 'river-trip', sender: 'Alice', timestamp: '2026-10-27T12:00:00Z', text: 'Dinner at six, by the river.', attachments: [], data: { source_revision: 1 } } }];
  await page.getByLabel('Recovery activity.json file', { exact: true }).setInputFiles({ name: 'activity.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(activity)) });
  await click('Import activity');
  const sourceItems = page.getByRole('region', { name: 'Source items' });
  await sourceItems.getByRole('button').first().waitFor(); await sourceItems.getByRole('button').first().click();
  await page.getByLabel('Local note', { exact: true }).waitFor(); await fill('Local note', 'Bring a notebook.'); await fill('Item tags', 'weekend');
  await click('Save local organization');
  await fill('Reply text', 'Thanks, see you there.'); await click('Preview reply');
  await page.getByLabel('I approve this exact reply for export', { exact: true }).check();
  const reply = JSON.parse((await downloaded(() => click('Export approved reply'))).bytes.toString());
  assert.equal(reply.delivery, 'not-sent'); assert.equal(reply.recipient, 'Alice'); assert.equal(reply.text, 'Thanks, see you there.');

  const { publicKey } = generateKeyPairSync('ed25519');
  const raw = publicKey.export({ format: 'der', type: 'spki' }).subarray(-32), alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let number = BigInt(`0x${raw.toString('hex')}`), encoded = '';
  while (number) { encoded = alphabet[Number(number % 58n)] + encoded; number /= 58n; }
  for (const byte of raw) { if (byte) break; encoded = '1' + encoded; }
  const audience = `did:sol:${encoded}`;
  await go('App access'); await fill('App or agent DID', audience); await fill('Collection', 'pub.tangled.note');
  await click('Issue grant');
  const certificate = JSON.parse((await downloaded(() => click('Download new certificate'))).bytes.toString());
  assert.equal(certificate.capability.aud, audience);
  await page.getByText('Certificate and revocation', { exact: true }).click();
  await click('Revoke grant');
  await page.getByText(/Revoked/).first().waitFor();

  await api('records', { action: 'put', collection: 'pub.example.fieldNotes', rkey: 'river', body: { title: 'River survey', text: 'Willows along the bank.', 'vendor.example': { keep: true } } });
  await go('Publish'); await page.getByRole('checkbox', { name: 'Select Weekend plans from pub.tangled.note', exact: true }).check();
  await page.getByRole('checkbox', { name: 'Select Slow reading from pub.tangled.capture', exact: true }).check();
  await page.getByRole('checkbox', { name: 'Include saved article text', exact: true }).check();
  await page.getByLabel('Share these selected items', { exact: true }).check(); await click('Preview selected content');
  await page.getByRole('heading', { name: 'Sharing preview', exact: true }).waitFor();
  const html = (await downloaded(() => click('Download index.html'))).bytes.toString(); assert.match(html, /Walk by the river/); assert.match(html, /Orchid fieldnotes/); assert.doesNotMatch(html, /Bring the field journal/); assert.doesNotMatch(html, /vendor.example/);
  await fill('Stable publication name', 'weekend-reading');
  await page.getByLabel('I reviewed this preview. Create or update a separate public copy under this name.', { exact: true }).check();
  await click('Create or update public publication');
  await page.getByText(/Public publication saved locally/).waitFor();

  await go('Recovery'); await fill('New recovery passphrase', 'local-fixture-recovery-passphrase'); await fill('Confirm recovery passphrase', 'local-fixture-recovery-passphrase');
  const backup = await downloaded(() => click('Download encrypted recovery copy'));
  assert.doesNotMatch(backup.bytes.toString(), /Walk by the river/); assert.equal(await page.getByLabel('New recovery passphrase', { exact: true }).inputValue(), '');
  await page.getByLabel('Encrypted recovery file', { exact: true }).setInputFiles({ name: 'recovery.json', mimeType: 'application/json', buffer: backup.bytes });
  await fill('Recovery passphrase', 'local-fixture-recovery-passphrase'); await click('Verify recovery copy');
  await page.getByText(/Recovery copy verified: decryption/).waitFor(); assert.equal(await page.getByLabel('Recovery passphrase', { exact: true }).inputValue(), '');
  await fill('Recovery passphrase', 'local-fixture-recovery-passphrase'); await fill('Recovered library name', 'Browser verified');
  await page.getByLabel('Create a fresh full-owner copy and switch to it', { exact: true }).check(); await click('Restore fresh library');
  await page.getByRole('heading', { name: 'Library', level: 1, exact: true }).waitFor();
  await fill('Search your library', 'Willows'); await click('Search'); await page.getByRole('button', { name: 'River survey', exact: true }).waitFor();
  const external = await api('records', { action: 'get', collection: 'pub.example.fieldNotes', rkey: 'river' }); assert.equal(external.body['vendor.example'].keep, true);

  if (screenshots) {
    await fs.mkdir(screenshots, { recursive: true });
    await fill('Search your library', 'Orchid'); await click('Search'); await click('Slow reading'); await page.getByText('Orchid fieldnotes for a quiet weekend. Saved here to read offline.', { exact: true }).waitFor();
    await page.locator('#notice').evaluate(node => { node.hidden = true; });
    await page.screenshot({ path: path.join(screenshots, 'personal-desktop.png'), fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    for (const label of ['Today','Library','Capture','Notes','Reading','Connections','App access','Recovery','Publish']) {
      await go(label);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
      assert.equal(overflow, false, `${label} overflows a mobile viewport`);
      if (label === 'Notes') { await click('Weekend plans'); await page.getByLabel('Text', { exact: true }).filter({ visible: true }).waitFor(); await page.waitForFunction(() => document.activeElement?.contains(document.querySelector('textarea')) && document.activeElement.getBoundingClientRect().top < 2); await page.screenshot({ path: path.join(screenshots, 'personal-mobile.png') }); }
    }
  }
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ workflows: 8, persisted: true, thirdPartyRecords: true, encryptedRecovery: true, noSourceDelivery: true, javascriptErrors: 0 }));
} finally {
  await browser?.close(); app.kill('SIGTERM');
  await new Promise(resolve => app.exitCode !== null ? resolve() : app.once('exit', resolve));
  await fs.rm(directory, { recursive: true, force: true });
}
