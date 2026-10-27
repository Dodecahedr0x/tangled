import test from 'node:test';
import assert from 'node:assert/strict';

// Minimal DOM for behavior tests; browser walkthrough checks native rendering.
class Element {
  constructor(tag = '') { this.tagName = tag.toUpperCase(); this.childNodes = []; this.listeners = {}; this.attributes = {}; this.value = ''; this.disabled = false; this.checked = false; this._text = ''; this.id = ''; this.htmlFor = ''; }
  set textContent(value) { this.childNodes = []; this._text = String(value); }
  get textContent() { return this._text + this.childNodes.map(node => node.textContent).join(''); }
  append(...nodes) { for (const node of nodes) { if (node.parentNode) node.remove(); node.parentNode = this; this.childNodes.push(node); } }
  replaceChildren(...nodes) { this.childNodes.forEach(node => { node.parentNode = null; }); this.childNodes = []; this._text = ''; this.append(...nodes); }
  setAttribute(key, value) { this.attributes[key] = value; }
  removeAttribute(key) { delete this.attributes[key]; }
  addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
  remove() { if (this.parentNode) this.parentNode.childNodes.splice(this.parentNode.childNodes.indexOf(this), 1); this.parentNode = null; }
  async fire(name) { for (const handler of this.listeners[name] || []) await handler({ preventDefault() {}, stopPropagation() {} }); await settle(); }
}
globalThis.Node = Element;
globalThis.document = { createElement: tag => new Element(tag), createTextNode: text => { const node = new Element(); node.textContent = text; return node; } };
globalThis.window = { dispatchEvent() {} };
const { mountAgents, mountDevices, mountPublish } = await import('../web/manage.js');
const settle = () => new Promise(resolve => setTimeout(resolve, 0));
function all(root) { return [root, ...root.childNodes.flatMap(all)]; }
function find(root, predicate) { const result = all(root).find(predicate); assert.ok(result, 'Expected control exists'); return result; }
function control(root, label) { const item = find(root, node => node.tagName === 'LABEL' && node.textContent === label); return find(root, node => node.id === item.htmlFor); }
function action(root, name) { return find(root, node => node.tagName === 'BUTTON' && node.textContent === name); }
function form(root, name) { let node = action(root, name); while (node && node.tagName !== 'FORM') node = node.parentNode; assert.ok(node); return node; }
function checkbox(root, text) { return find(root, node => node.tagName === 'LABEL' && node.textContent === text).childNodes[0]; }
function context(api = async () => ({}), request = async () => ({})) {
  const calls = [], notices = [], downloads = [], navigations = [];
  return { calls, notices, downloads, navigations, session: { data: '/private/recovered' }, async api(feature, input) { calls.push([feature, input]); return api(feature, input); }, async request(path, input) { calls.push([path, input]); return request(path, input); }, notify(...args) { notices.push(args); }, download(...args) { downloads.push(args); }, async run(node, task) { if (node.disabled) return; node.disabled = true; try { await task(); } catch (error) { notices.push([error.message, 'error']); } finally { node.disabled = false; } }, async refreshSession() { calls.push(['refreshSession']); }, navigate(view) { navigations.push(view); } };
}
const record = { uri: 'tangled://owner/pub.example.story/one', collection: 'pub.example.story', rkey: 'one', vis: 'cap', ts: 1, body: { title: '<Private story>', text: 'An ordinary private record', blob: 'b-original' } };

test('App access issues a scoped certificate and shows revocations and cited receipts', async () => {
  const root = new Element('section');
  const capability = { aud: 'did:sol:agent', cmd: 'repo/read', res: 'tangled://owner/pub.tangled.note', exp: Math.floor(Date.now() / 1000) + 3600 };
  const ctx = context(async (feature, input) => {
    if (feature === 'agents' && input.action === 'list') return { grants: [{ uri: 'grant-uri', body: { cid: 'b-grant', capability } }] };
    if (feature === 'agents' && input.action === 'report') return { activities: [{ body: { actor: 'did:sol:agent', action: 'search', request: { query: 'orchid', collection: 'pub.tangled.note' }, invocation_cbor: 'SECRET_HEX', citations: [{ uri: record.uri, cid: 'b-result' }] } }] };
    if (feature === 'agents' && input.action === 'grant') return { cid: 'b-new', capability };
    if (input.collection === 'sys.revocation') return { records: [{ body: { cid: 'b-grant', reason: 'finished' } }] };
    return { records: [record] };
  });
  await mountAgents(root, ctx);
  assert.match(root.textContent, /Revoked/); assert.match(root.textContent, /orchid/); assert.match(root.textContent, /b-result/); assert.doesNotMatch(root.textContent, /SECRET_HEX/);
  control(root, 'App or agent DID').value = 'did:sol:agent'; control(root, 'Collection').value = 'pub.tangled.note'; control(root, 'Access').value = 'read';
  await form(root, 'Issue grant').fire('submit');
  assert.deepEqual(ctx.calls.find(([feature, body]) => feature === 'agents' && body?.action === 'grant')[1], { action: 'grant', audience: 'did:sol:agent', collection: 'pub.tangled.note', access: 'read', ttl_seconds: 3600 });
  await action(root, 'Download new certificate').fire('click');
  assert.deepEqual(ctx.downloads[0][1], { cid: 'b-new', capability });
});

test('Recovery clears passwords on export failure and passes encrypted files to verify/restore', async () => {
  const root = new Element('section'); let failExport = true;
  const packet = { encrypted: 'opaque', owner: 'did:sol:owner' };
  const ctx = context(async () => ({ owner: 'did:sol:owner', records: 2, blobs: 3 }), async path => {
    if (path === 'backups/export' && failExport) throw new Error('Disk unavailable. Retry.');
    if (path === 'backups/export') return { filename: 'recovery.json', backup: packet };
    return { session: { data: '/private/restored' } };
  });
  await mountDevices(root, ctx);
  control(root, 'New recovery passphrase').value = control(root, 'Confirm recovery passphrase').value = 'strong password here';
  await form(root, 'Download encrypted recovery copy').fire('submit');
  assert.equal(control(root, 'New recovery passphrase').value, ''); assert.equal(control(root, 'Confirm recovery passphrase').value, '');
  failExport = false; control(root, 'New recovery passphrase').value = control(root, 'Confirm recovery passphrase').value = 'strong password here';
  await form(root, 'Download encrypted recovery copy').fire('submit'); assert.deepEqual(ctx.downloads[0], ['recovery.json', packet]);
  control(root, 'Encrypted recovery file').files = [{ size: 100, async text() { return JSON.stringify(packet); } }];
  control(root, 'Recovery passphrase').value = 'strong password here'; await action(root, 'Verify recovery copy').fire('click');
  assert.equal(control(root, 'Recovery passphrase').value, ''); assert.deepEqual(ctx.calls.find(([path]) => path === 'backups/verify')[1].backup, packet);
  control(root, 'Recovery passphrase').value = 'strong password here'; await action(root, 'Restore fresh library').fire('click');
  assert.ok(!ctx.calls.some(([path]) => path === 'backups/restore')); assert.equal(control(root, 'Recovery passphrase').value, '');
  control(root, 'Recovery passphrase').value = 'strong password here'; checkbox(root, 'Create a fresh full-owner copy and switch to it').checked = true;
  await action(root, 'Restore fresh library').fire('click');
  assert.equal(ctx.calls.find(([path]) => path === 'backups/restore')[1].label, 'Recovered library'); assert.deepEqual(ctx.navigations, ['library']);
  assert.ok(ctx.calls.some(([path]) => path === 'refreshSession')); assert.match(ctx.notices.at(-1)[0], /\/private\/restored/);
  assert.doesNotMatch(root.textContent, /strong password here|opaque/);
});

test('Recovery rejects invalid files and clears the passphrase on verify error', async () => {
  const root = new Element('section'); const ctx = context(async () => ({})); await mountDevices(root, ctx);
  control(root, 'Encrypted recovery file').files = [{ size: 10, async text() { return 'bad JSON'; } }];
  control(root, 'Recovery passphrase').value = 'strong password here'; await action(root, 'Verify recovery copy').fire('click');
  assert.equal(control(root, 'Recovery passphrase').value, ''); assert.ok(!ctx.calls.some(([path]) => path === 'backups/verify')); assert.match(root.textContent, /original encrypted backup/);
});

test('Publish requires selection and explicit share, preserves unknown collections, and confirms public copy separately', async () => {
  const root = new Element('section'); const ctx = context(async (feature, body) => feature === 'records' ? { records: [record] } : { html: '<p>Escaped preview</p>', rss: '<rss/>', count: 1, files: [{ path: 'attachments/original.bin', bytes: [0, 255, 12], mediaType: 'application/octet-stream' }], publication: { uri: 'public-uri', cid: 'public-cid' } });
  await mountPublish(root, ctx);
  const select = find(root, node => node.attributes['aria-label']?.startsWith('Select ')); select.checked = true; await select.fire('change');
  await form(root, 'Preview selected content').fire('submit'); assert.ok(!ctx.calls.some(([feature]) => feature === 'publish'));
  const share = checkbox(root, 'Share these selected items'); share.checked = true; await share.fire('change');
  await form(root, 'Preview selected content').fire('submit');
  const render = ctx.calls.find(([feature]) => feature === 'publish')[1]; assert.deepEqual(render.records, [{ collection: 'pub.example.story', rkey: 'one' }]); assert.equal(render.confirmPublish, true);
  const iframe = find(root, node => node.tagName === 'IFRAME'); assert.equal(iframe.attributes.sandbox, '');
  assert.equal(action(root, 'Create or update public publication').disabled, true);
  await action(root, 'Download attachments/original.bin').fire('click'); assert.deepEqual(ctx.downloads[0][1], new Uint8Array([0, 255, 12]));
  control(root, 'Stable publication name').value = 'selected-reading';
  const final = checkbox(root, 'I reviewed this preview. Create or update a separate public copy under this name.'); final.checked = true; await final.fire('change');
  await action(root, 'Create or update public publication').fire('click'); assert.equal(ctx.calls.at(-1)[1].key, 'selected-reading'); assert.equal(ctx.calls.at(-1)[1].action, 'publish');
  control(root, 'Sharing title').value = 'Changed'; await control(root, 'Sharing title').fire('input'); assert.equal(action(root, 'Create or update public publication').disabled, true);
  assert.ok(!ctx.calls.some(([feature, body]) => feature === 'records' && body.action !== 'list'));
});

test('Publish ignores a preview returned after selection changed', async () => {
  const root = new Element('section'); let finish;
  const ctx = context(async (feature) => feature === 'records' ? { records: [record] } : new Promise(resolve => { finish = resolve; }));
  await mountPublish(root, ctx);
  const select = find(root, node => node.attributes['aria-label']?.startsWith('Select ')); select.checked = true; await select.fire('change');
  checkbox(root, 'Share these selected items').checked = true;
  const pending = form(root, 'Preview selected content').fire('submit'); await settle();
  select.checked = false; await select.fire('change'); finish({ html: '<p>old</p>' }); await pending;
  assert.equal(action(root, 'Create or update public publication').disabled, true); assert.ok(!all(root).some(node => node.tagName === 'IFRAME'));
});

test('Publish adds attachments only when explicitly chosen and preserves binary downloads', async () => {
  const root = new Element('section');
  const ctx = context(async feature => feature === 'records' ? { records: [record] } : { html: '<p>Preview</p>', rss: '<rss/>', count: 1 });
  await mountPublish(root, ctx);
  const chosen = find(root, node => node.attributes['aria-label']?.startsWith('Select ')); chosen.checked = true; await chosen.fire('change');
  control(root, 'Attachment CID to include').value = 'b-explicit'; control(root, 'Attachment download name').value = 'Diagram'; await action(root, 'Add this attachment').fire('click');
  control(root, 'Use this UTF-8 text blob').value = 'b-text'; await control(root, 'Use this UTF-8 text blob').fire('input');
  checkbox(root, 'Share these selected items').checked = true;
  await form(root, 'Preview selected content').fire('submit');
  assert.deepEqual(ctx.calls.at(-1)[1].records[0], { collection: 'pub.example.story', rkey: 'one', textBlob: 'b-text', attachments: [{ cid: 'b-explicit', name: 'Diagram' }] });
  assert.ok(!JSON.stringify(ctx.calls.at(-1)[1]).includes('b-original'));
});

test('Recovery sync requires an explicit import decision and clears secrets on server errors', async () => {
  const root = new Element('section');
  const ctx = context(async () => ({}), async path => { if (path === 'backups/sync') throw new Error('History diverged. Restore a fresh library instead.'); return {}; });
  await mountDevices(root, ctx);
  control(root, 'Encrypted recovery file').files = [{ size: 20, async text() { return '{"encrypted":"fixture"}'; } }];
  control(root, 'Recovery passphrase').value = 'strong password here'; await action(root, 'Sync newer copy').fire('click');
  assert.ok(!ctx.calls.some(([path]) => path === 'backups/sync')); assert.equal(control(root, 'Recovery passphrase').value, '');
  checkbox(root, 'Import this copy’s verified newer history into the current library').checked = true;
  control(root, 'Recovery passphrase').value = 'strong password here'; await action(root, 'Sync newer copy').fire('click');
  assert.ok(ctx.calls.some(([path]) => path === 'backups/sync')); assert.equal(control(root, 'Recovery passphrase').value, ''); assert.match(root.textContent, /History diverged/);
});

const capturedArticle = { ...record, uri: 'tangled://owner/pub.tangled.capture/article', collection: 'pub.tangled.capture', rkey: 'article', body: { kind: 'article', title: 'Saved article', mime: 'text/plain', blob: 'b-article-text', url: 'https://example.org/original', source: { private: { cid: 'b-hidden-original' } } } };
const attachedNote = { ...record, uri: 'tangled://owner/pub.tangled.note/note', collection: 'pub.tangled.note', rkey: 'note', body: { title: 'Note with diagram', text: 'Inline note', attachments: [{ cid: 'b-diagram', name: 'diagram.png', mime: 'image/png' }] } };
function selectRecord(root, title) { return find(root, node => node.attributes['aria-label']?.startsWith(`Select ${title} from `)); }
async function selectAndShare(root, title) { const chosen = selectRecord(root, title); chosen.checked = true; await chosen.fire('change'); checkbox(root, 'Share these selected items').checked = true; }

test('Selecting a saved article includes its named text by default without copying original URLs or hidden fields', async () => {
  const root = new Element('section'); const ctx = context(async feature => feature === 'records' ? { records: [capturedArticle, attachedNote] } : { html: '<p>Article body</p>', rss: '<rss/>', count: 1 });
  await mountPublish(root, ctx);
  assert.equal(checkbox(root, 'Include saved article text').checked, true);
  await selectAndShare(root, 'Saved article'); await form(root, 'Preview selected content').fire('submit');
  assert.deepEqual(ctx.calls.at(-1)[1].records, [{ collection: 'pub.tangled.capture', rkey: 'article', textBlob: 'b-article-text' }]);
  assert.ok(!root.textContent.includes('b-hidden-original')); assert.ok(!JSON.stringify(ctx.calls.at(-1)).includes('b-diagram'));
  const includeText = checkbox(root, 'Include saved article text'); includeText.checked = false; await includeText.fire('change');
  assert.equal(checkbox(root, 'I reviewed this preview. Create or update a separate public copy under this name.').disabled, true);
  await form(root, 'Preview selected content').fire('submit'); assert.ok(!ctx.calls.at(-1)[1].records[0].textBlob);
  const includeLink = checkbox(root, 'Include original web link'); includeLink.checked = true; await includeLink.fire('change');
  await form(root, 'Preview selected content').fire('submit'); assert.equal(ctx.calls.at(-1)[1].records[0].link, capturedArticle.body.url);
});

test('Named note attachments and captured files stay excluded until explicitly checked and invalidate previews', async () => {
  const file = { ...record, uri: 'tangled://owner/pub.tangled.capture/file', collection: 'pub.tangled.capture', rkey: 'file', body: { kind: 'file', title: 'report.pdf', mime: 'application/pdf', blob: 'b-file' } };
  const root = new Element('section'); const ctx = context(async feature => feature === 'records' ? { records: [attachedNote, file] } : { html: '<p>Preview</p>', rss: '<rss/>', count: 2 });
  await mountPublish(root, ctx); assert.equal(checkbox(root, 'Include diagram.png').checked, false); assert.equal(checkbox(root, 'Include report.pdf').checked, false);
  await selectAndShare(root, 'Note with diagram'); await selectAndShare(root, 'report.pdf'); await form(root, 'Preview selected content').fire('submit');
  assert.ok(ctx.calls.at(-1)[1].records.every(item => !item.attachments));
  const diagram = checkbox(root, 'Include diagram.png'); diagram.checked = true; await diagram.fire('change');
  assert.ok(!all(root).some(node => node.tagName === 'IFRAME'));
  await form(root, 'Preview selected content').fire('submit'); assert.deepEqual(ctx.calls.at(-1)[1].records[0].attachments, [{ cid: 'b-diagram', name: 'diagram.png' }]);
  diagram.checked = false; await diagram.fire('change'); await form(root, 'Preview selected content').fire('submit'); assert.ok(!ctx.calls.at(-1)[1].records[0].attachments);
  const original = checkbox(root, 'Include report.pdf'); original.checked = true; await original.fire('change'); await form(root, 'Preview selected content').fire('submit'); assert.deepEqual(ctx.calls.at(-1)[1].records[1].attachments, [{ cid: 'b-file', name: 'report.pdf' }]);
});

test('Canonical article profile gets named text selection while future profiles retain advanced explicit references', async () => {
  const article = { ...record, uri: 'tangled://owner/pub.tangled.article/canonical', collection: 'pub.tangled.article', rkey: 'canonical', body: { title: 'Canonical article', blob: 'b-canonical' } };
  const root = new Element('section'); const ctx = context(async feature => feature === 'records' ? { records: [article] } : { html: '<p>Preview</p>', rss: '<rss/>', count: 1 });
  await mountPublish(root, ctx); await selectAndShare(root, 'Canonical article'); await form(root, 'Preview selected content').fire('submit');
  assert.equal(ctx.calls.at(-1)[1].records[0].textBlob, 'b-canonical');
  control(root, 'Use this UTF-8 text blob').value = 'b-explicit-override'; await control(root, 'Use this UTF-8 text blob').fire('input');
  await form(root, 'Preview selected content').fire('submit'); assert.equal(ctx.calls.at(-1)[1].records[0].textBlob, 'b-explicit-override');
});

test('Changing a named saved-text selection while rendering rejects the stale preview', async () => {
  const root = new Element('section'); let finish;
  const ctx = context(async feature => feature === 'records' ? { records: [capturedArticle] } : new Promise(resolve => { finish = resolve; }));
  await mountPublish(root, ctx); await selectAndShare(root, 'Saved article');
  const pending = form(root, 'Preview selected content').fire('submit'); await settle();
  const include = checkbox(root, 'Include saved article text'); include.checked = false; await include.fire('change'); finish({ html: '<p>Previously selected article</p>' }); await pending;
  assert.ok(!all(root).some(node => node.tagName === 'IFRAME')); assert.equal(action(root, 'Create or update public publication').disabled, true);
});

test('Unknown or non-text article profiles never infer original attachments or text', async () => {
  const unknown = { ...record, body: { title: 'Future profile', blob: 'b-unknown', attachments: [{ cid: 'b-private', name: 'Private original' }], source: { cid: 'b-nested-private' }, url: 'https://user:secret@example.org/' } };
  const binaryArticle = { ...record, collection: 'pub.tangled.article', rkey: 'binary', uri: 'tangled://owner/pub.tangled.article/binary', body: { title: 'Binary article', mime: 'application/octet-stream', blob: 'b-binary' } };
  const root = new Element('section'); const ctx = context(async feature => feature === 'records' ? { records: [unknown, binaryArticle] } : { html: '<p>Preview</p>', count: 2 });
  await mountPublish(root, ctx); await selectAndShare(root, 'Future profile'); await selectAndShare(root, 'Binary article');
  await form(root, 'Preview selected content').fire('submit');
  assert.ok(ctx.calls.at(-1)[1].records.every(item => !item.textBlob && !item.attachments && !item.link));
  assert.ok(!root.textContent.includes('b-nested-private')); assert.ok(!root.textContent.includes('user:secret'));
});
