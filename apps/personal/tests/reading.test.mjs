import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

class Element {
  constructor(tag, attrs = {}, children = []) {
    this.tagName = tag; this.children = []; this.value = ''; this.checked = false;
    Object.assign(this, attrs); this.append(...children);
  }
  append(...children) { this.children.push(...children.flat(Infinity).filter(x => x != null)); }
  replaceChildren(...children) { this.children = []; this.append(...children); }
  setAttribute(key, value) { this[key] = value; }
  removeAttribute(key) { delete this[key]; }
  addEventListener(name, callback) { this['on' + name[0].toUpperCase() + name.slice(1)] = callback; }
  focus() {}
  get textContent() { return (this.text || '') + this.children.map(x => typeof x === 'object' ? x.textContent : x).join(' '); }
}
const el = (tag, attrs = {}, ...children) => new Element(tag, attrs, children);
globalThis.readingTestHelpers = {
  el, field: (label, control, hint) => el('label', {}, label, control, hint),
  button: (label, onClick, kind) => el('button', { onClick, text: label, class: kind }),
  empty: (title, detail, action) => el('section', {}, title, detail, action),
  recordTitle: r => r.body?.title || r.body?.text || 'Untitled', timeLabel: t => String(t),
};
const source = (await readFile(new URL('../web/reading.js', import.meta.url), 'utf8'))
  .replace(/^import .*from '\.\/ui\.js';/m, 'const {el,field,button,empty,recordTitle,timeLabel} = globalThis.readingTestHelpers;');
const { mountFeeds, mountConnections } = await import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));
const nodes = root => [root, ...root.children.flatMap(x => typeof x === 'object' ? nodes(x) : [])];
const byText = (root, text) => nodes(root).find(x => x.tagName === 'button' && x.text === text);
const control = (root, label) => nodes(root).find(x => x.tagName === 'label' && x.children[0] === label)?.children.find(x => typeof x === 'object');
function context(api) {
  const calls = [], errors = [], downloads = [];
  return { calls, errors, downloads, api: async (feature, input) => { calls.push([feature, input]); return api(feature, input); },
    notify: (message, tone) => { if (tone === 'error') errors.push(message); },
    run: async (node, work) => { try { await work(); } catch (error) { errors.push(error.message); } },
    download: (...args) => downloads.push(args),
  };
}

test('feeds import uses the chosen subscription and rejects an oversized file before reading it', async () => {
  const root = el('main');
  const ctx = context((_, input) => input.action === 'list' ? { subscriptions: [
    { body: { url: 'https://a.test/feed', title: 'A', tags: [] } },
    { body: { url: 'https://b.test/feed', title: 'B', tags: [] } },
  ] } : input.action === 'queue' ? { items: [] } : { imported: 1, skipped: 0 });
  await mountFeeds(root, ctx);
  const file = control(root, 'RSS or Atom file');
  assert.ok(file, 'file picker is available');
  const subscription = control(root, 'Import into subscription');
  subscription.value = 'https://b.test/feed';
  file.files = [new File(['<feed/>'], 'feed.xml')];
  await byText(root, 'Import feed file').onClick();
  assert.deepEqual(ctx.calls.find(([, i]) => i.action === 'ingest')?.[1], { action: 'ingest', url: 'https://b.test/feed', xml: '<feed/>' });
  file.files = [{ size: 2097153, text: () => { throw Error('must not read'); } }];
  await byText(root, 'Import feed file').onClick();
  assert.match(ctx.errors.at(-1), /2 MiB/);
});

test('connection replies bind native identity and editing invalidates approval', async () => {
  const item = { rkey: 'one', uri: 'tangled://owner/sourceItem/one', cid: 'source-cid', ts: 1,
    body: { imported: { source: 'telegram', account: 'account-7', object: { id: '42', kind: 'message', sender: 'alice-9', conversation: 'room', text: 'Hello', timestamp: 'native time' } }, revisions: [], currentRevision: 0 } };
  const ctx = context((_, input) => {
    if (input.action === 'list' || input.action === 'search') return [item];
    if (input.action === 'contacts' || input.action === 'history') return [];
    if (input.action === 'get') return item;
    if (input.action === 'reply-preview') return { cid: 'preview-cid', body: { reply: { source: 'telegram', account: 'account-7', recipient: 'alice-9', text: input.text, conversation: 'room', inReplyTo: '42', kind: 'message', targetUri: item.uri, targetCid: item.cid, status: 'draft' } } };
    if (input.action === 'reply-export') return { delivery: 'not-sent' };
    throw Error(input.action);
  });
  const root = el('main'); await mountConnections(root, ctx);
  await byText(root, 'Hello').onClick();
  const draft = control(root, 'Reply text'); draft.value = 'Thanks';
  await byText(root, 'Preview reply').onClick();
  assert.deepEqual(ctx.calls.find(([, i]) => i.action === 'reply-preview')?.[1], { action: 'reply-preview', rkey: 'one', account: 'account-7', recipient: 'alice-9', text: 'Thanks' });
  const approve = control(root, 'I approve this exact reply for export'); approve.checked = true; approve.onChange();
  assert.equal(byText(root, 'Export approved reply').disabled, false);
  draft.value = 'Changed'; draft.onInput();
  assert.equal(byText(root, 'Export approved reply').disabled, true);
  await byText(root, 'Preview reply').onClick();
  const approved = control(root, 'I approve this exact reply for export'); approved.checked = true; approved.onChange();
  await byText(root, 'Export approved reply').onClick();
  assert.equal(ctx.downloads.at(-1)[1].delivery, 'not-sent');
  assert.deepEqual(ctx.calls.find(([, i]) => i.action === 'reply-export')?.[1], { action: 'reply-export', rkey: 'one', previewCid: 'preview-cid', confirmed: true });
});

test('activity file preview reports sources and failed imports preserve the selected file', async () => {
  let reject = true;
  const ctx = context((_, input) => {
    if (input.action === 'list' || input.action === 'contacts') return [];
    if (input.action === 'ingest') { if (reject) throw Error('Storage unavailable. Retry import.'); return { items: [{}] }; }
  });
  const root = el('main'); await mountConnections(root, ctx);
  const picker = control(root, 'Recovery activity.json file');
  const activity = [{ source: 'whatsapp', account: 'me', object: { id: '1', kind: 'message', text: 'Hello' } }];
  const file = new File([JSON.stringify(activity)], 'activity.json'); picker.files = [file];
  await picker.onChange();
  assert.match(root.textContent, /1 entries.*whatsapp \(1\)/);
  assert.equal(byText(root, 'Import activity').disabled, false);
  await byText(root, 'Import activity').onClick();
  assert.match(ctx.errors.at(-1), /Storage unavailable/);
  assert.equal(picker.files[0], file);
  reject = false; await byText(root, 'Import activity').onClick();
  assert.deepEqual(ctx.calls.filter(([, i]) => i.action === 'ingest').at(-1)[1].activity, activity);
});

test('source originals download exact bytes for the chosen revision', async () => {
  const item = { rkey: 'r', ts: 1, body: { currentRevision: 1, imported: { source: 'test', account: 'me', object: { id: '1', kind: 'post', text: '<img src=x onerror=alert(1)>', sender: null } } } };
  const ctx = context((_, input) => {
    if (input.action === 'list') return [item];
    if (input.action === 'contacts') return [];
    if (input.action === 'get') return item;
    if (input.action === 'history') return [{ imported: { object: {} } }, { imported: { object: {} }, sourceOriginalBlob: 'blob' }];
    if (input.action === 'original') return { bytes: [0, 127, 128, 255], mime: 'application/octet-stream' };
  });
  const root = el('main'); await mountConnections(root, ctx);
  await byText(root, item.body.imported.object.text).onClick();
  assert.equal(nodes(root).some(node => node.tagName === 'img'), false);
  assert.equal(byText(root, 'Download original source bytes').disabled, false);
  await byText(root, 'Download original source bytes').onClick();
  assert.deepEqual([...ctx.downloads.at(-1)[1]], [0, 127, 128, 255]);
  assert.deepEqual(ctx.calls.find(([, i]) => i.action === 'original')[1], { action: 'original', rkey: 'r', index: 1, sourceOriginal: true });
});

test('reading state mutations retain the item URI and source HTML is inert', async () => {
  const item = { uri: 'tangled://owner/feedItem/a', ts: 1, body: { title: 'Article', body: '<script>alert(1)</script>', bodyType: 'html', sourceUrl: 'https://source.test/feed', link: 'javascript:alert(1)' }, reading: { read: false, starred: false, tags: [] } };
  const ctx = context((_, input) => input.action === 'list' ? { subscriptions: [] } : input.action === 'queue' ? { items: [item] } : {});
  const root = el('main'); await mountFeeds(root, ctx);
  await byText(root, 'Article').onClick();
  assert.equal(nodes(root).some(node => node.tagName === 'script' || node.href?.startsWith('javascript:')), false);
  await byText(root, 'Mark read').onClick();
  assert.deepEqual(ctx.calls.find(([, i]) => i.action === 'mark')[1], { action: 'mark', item: item.uri, read: true });
});

async function htmlArticleText(html) {
  const item = { uri: 'tangled://owner/feedItem/html', ts: 1, body: { title: 'HTML article', body: html, bodyType: 'html', sourceUrl: 'https://source.test/feed' }, reading: {} };
  const ctx = context((_, request) => request.action === 'list' ? { subscriptions: [] } : { items: [item] });
  const root = el('main'); await mountFeeds(root, ctx); await byText(root, 'HTML article').onClick();
  return { text: nodes(root).find(node => node.class === 'reader-body').textContent, item };
}

test('HTML feed articles read naturally without markup or active-content text', async () => {
  const html = '<p>Morning light across <b>orchids</b>.</p><script>alert("hidden")</script><style>.hidden {color:red}</style><p>Second paragraph.</p>';
  const { text, item } = await htmlArticleText(html);
  assert.equal(text, 'Morning light across orchids.\n\nSecond paragraph.');
  assert.equal(item.body.body, html, 'source evidence remains unchanged');
});

test('HTML plain text preserves entities and handles quoted tag delimiters', async () => {
  const { text } = await htmlArticleText('<p title="a > b">Tea &amp; cake&nbsp;&#8212; &#x1F337; &lt;3 &quot;hello&quot;</p><p>Line<br>two &copy; 2026 &Eacute; &eacute; &AMP; &constructor;</p>');
  assert.equal(text, 'Tea & cake — 🌷 <3 "hello"\n\nLine\ntwo © 2026 É é & &constructor;');
});

test('malformed HTML stays bounded and does not reveal unterminated script content', async () => {
  assert.equal((await htmlArticleText('<p>Visible</p><!-- no end <script>hidden')).text, 'Visible');
  assert.equal((await htmlArticleText('<p>Visible</p><ScRiPt data-x=">">hidden <b>text')).text, 'Visible');
  assert.equal((await htmlArticleText('<p>Visible</p><a title="unterminated > hidden')).text, 'Visible');
  const { text } = await htmlArticleText('<p>' + 'x'.repeat(300000) + '</p>');
  assert.ok(text.length <= 262144, 'reader extraction bounds oversized input');
  const multibyte = (await htmlArticleText('é'.repeat(262144))).text;
  assert.ok(new TextEncoder().encode(multibyte).length <= 262144, 'input is bounded by UTF-8 bytes');
});
