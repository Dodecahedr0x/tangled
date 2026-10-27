import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

class Element {
  constructor(tag, attrs = {}, children = []) {
    this.tagName = tag; this.children = []; this.value = ''; this.checked = false;
    Object.assign(this, attrs); this.append(...children);
  }
  append(...children) { this.children.push(...children.flat(Infinity).filter(x => x != null)); }
  replaceChildren(...children) { this.children = children.map(child => child instanceof Element ? child : String(child)); }
  setAttribute(key, value) { this[key] = value; }
  removeAttribute(key) { delete this[key]; }
  addEventListener(name, callback) { this['on' + name[0].toUpperCase() + name.slice(1)] = callback; }
  focus() {}
  get textContent() { return (this.text || '') + this.children.map(x => typeof x === 'object' ? x.textContent : x).join(' '); }
}
const el = (tag, attrs = {}, ...children) => new Element(tag, attrs, children);
const revealedSelections = [];
globalThis.readingTestHelpers = {
  revealSelection: node => revealedSelections.push(node),
  el, field: (label, control, hint) => el('label', {}, label, control, hint),
  button: (label, onClick, kind) => el('button', { onClick, text: label, class: kind }),
  empty: (title, detail, action) => el('section', {}, title, detail, action),
  recordTitle: r => r.body?.title || r.body?.text || 'Untitled', timeLabel: t => String(t),
};
const source = (await readFile(new URL('../web/reading.js', import.meta.url), 'utf8'))
  .replace(/^import .*from '\.\/ui\.js';/m, 'const {el,field,button,empty,recordTitle,timeLabel,revealSelection} = globalThis.readingTestHelpers;');
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
  assert.equal(nodes(root).find(node => node.class === 'reader').children.includes('null'), false, 'native reader omits absent conditional nodes');
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


test('subscription selects contain native option nodes rather than a stringified array', async () => {
  const ctx = context((_, request) => request.action === 'list' ? { subscriptions: [
    { body: { url: 'https://a.test/feed', title: 'A', tags: [] } },
    { body: { url: 'https://b.test/feed', title: 'B', tags: [] } },
  ] } : { items: [] });
  const root = el('main'); await mountFeeds(root, ctx);
  for (const label of ['Import into subscription', 'Subscription filter']) {
    const select = control(root, label);
    assert.equal(select.children.length, 3);
    assert.deepEqual(select.children.map(option => [option.tagName, option.value]), [
      ['option', ''], ['option', 'https://a.test/feed'], ['option', 'https://b.test/feed'],
    ]);
  }
});


test('reader reveal follows explicit item selection without following refresh or organization saves', async () => {
  const updates = [];
  const start = revealedSelections.length;
  const item = { uri: 'tangled://owner/feedItem/mobile', ts: 1, body: { title: 'Mobile article', body: 'Text', sourceUrl: 'https://source.test/feed' }, reading: {} };
  const ctx = context((_, request) => request.action === 'list' ? { subscriptions: [] } : request.action === 'queue' ? { items: [item] } : {});
  ctx.updateRecordLink = (...args) => updates.push(args);
  const root = el('main'); await mountFeeds(root, ctx);
  assert.equal(revealedSelections.length, start, 'mount does not reveal');
  assert.deepEqual(updates, [], 'mount does not rewrite the current URL');
  await byText(root, 'Mobile article').onClick();
  assert.equal(revealedSelections.length, start + 1);
  assert.equal(revealedSelections.at(-1).class, 'reader');
  await byText(root, 'Mark read').onClick();
  await byText(root, 'Apply filters').onClick();
  assert.equal(revealedSelections.length, start + 1, 'save and filter do not reveal');
  assert.deepEqual(updates, [['feeds', item.uri]], 'only explicit selection replaces the current record link');
});

test('contact filtering does not reveal a reader until an actual source item is opened', async () => {
  const updates = [];
  const start = revealedSelections.length;
  const item = { uri: 'tangled://owner/pub.tangled.sourceItem/mobile-source', rkey: 'mobile-source', ts: 1, body: { imported: { source: 'telegram', account: 'me', object: { id: '1', kind: 'message', sender: 'alice', text: 'Mobile message' } }, currentRevision: 0 } };
  const contact = { uri: 'tangled://owner/contact/alice', body: { identity: { source: 'telegram', account: 'me', sender: 'alice' }, alias: 'Alice', groups: [] } };
  const ctx = context((_, request) => request.action === 'contacts' ? [contact] : request.action === 'history' ? [] : request.action === 'get' ? item : [item]);
  ctx.updateRecordLink = (...args) => updates.push(args);
  const root = el('main'); await mountConnections(root, ctx);
  assert.equal(revealedSelections.length, start);
  assert.deepEqual(updates, []);
  await byText(root, 'Show contact items').onClick();
  assert.equal(revealedSelections.length, start, 'contact filtering does not reveal');
  assert.deepEqual(updates, [], 'contact filtering does not change the selected record URL');
  await byText(root, 'Mobile message').onClick();
  assert.equal(revealedSelections.length, start + 1);
  assert.equal(revealedSelections.at(-1).class, 'reader');
  await byText(root, 'Save local organization').onClick();
  assert.equal(revealedSelections.length, start + 1, 'organization save does not reveal');
  assert.deepEqual(updates, [['connections', item.uri]], 'only opening the source item updates the record link');
});

const linkedFeed = { uri: 'tangled://did:sol:local/pub.tangled.feedItem/linked', collection: 'pub.tangled.feedItem', rkey: 'linked', ts: 1,
  body: { title: 'Linked article', body: 'Outside the queue', sourceUrl: 'https://source.test/feed', extra: { retained: true } } };
const linkedSource = { uri: 'tangled://did:sol:local/pub.tangled.sourceItem/linked', collection: 'pub.tangled.sourceItem', rkey: 'linked', ts: 1,
  body: { imported: { source: 'third-party', account: 'me', object: { id: '7', kind: 'message', text: 'Linked source', sender: 'alice' } }, currentRevision: 0, deleted: true, extra: { retained: true } } };
function linkedContext(item, overrides = {}) {
  const ctx = context((feature, request) => {
    if (feature === 'info') return { did: 'did:sol:local' };
    if (feature === 'records' && request.action === 'get') return item;
    if (feature === 'records' && request.collection === 'pub.tangled.readingState') return { records: [{ body: { item: linkedFeed.uri, read: true, starred: true, tags: ['kept'] } }] };
    if (feature === 'feeds') return request.action === 'list' ? { subscriptions: [] } : { items: [] };
    if (feature === 'connections') return request.action === 'get' ? item : [];
    throw Error(`Unexpected API ${feature}/${request.action}`);
  });
  return Object.assign(ctx, { selection: { uri: item.uri } }, overrides);
}

for (const [view, mount, item, readerText] of [['feeds', mountFeeds, linkedFeed, 'Outside the queue'], ['connections', mountConnections, linkedSource, 'Linked source']]) {
  test(`${view} opens its exact linked record outside filters without writes and copies its stable link`, async () => {
    const before = JSON.stringify(item), copied = [], updates = [], root = el('main');
    const ctx = linkedContext(item, { copyRecordLink: async (...args) => copied.push(args), updateRecordLink: (...args) => updates.push(args) });
    await mount(root, ctx);
    assert.ok(root.textContent.includes(readerText), 'linked content is rendered despite an empty list');
    assert.deepEqual(updates, [], 'opening the initial route does not replace its URL');
    if (view === 'feeds') {
      assert.ok(byText(root, 'Mark unread'), 'existing read state is retained');
      assert.ok(byText(root, 'Unstar article'));
      assert.equal(control(root, 'Article tags').value, 'kept');
      assert.deepEqual(ctx.calls.find(([feature, request]) => feature === 'records' && request.action === 'get')?.[1], { action: 'get', collection: 'pub.tangled.feedItem', rkey: 'linked' });
    }
    assert.ok(ctx.calls.every(([, request]) => !['mark', 'overlay', 'put', 'contact', 'select-revision'].includes(request.action)), 'opening never mutates signed records');
    await byText(root, 'Copy link').onClick();
    assert.deepEqual(copied, [[view, item.uri]]);
    ctx.copyRecordLink = async () => { throw Error('Clipboard unavailable. Copy the address manually.'); };
    await byText(root, 'Copy link').onClick();
    assert.match(ctx.errors.at(-1), /Clipboard unavailable/);
    assert.equal(JSON.stringify(item), before, 'source record and extension fields are untouched');
  });

  test(`${view} rejects malformed, wrong-collection, foreign and unavailable record links without writes`, async () => {
    for (const uri of ['not-a-uri', 'tangled://did:sol:local/pub.tangled.note/linked', item.uri.replace('did:sol:local', 'did:sol:foreign'), item.uri]) {
      const ctx = linkedContext(item, { selection: { uri } }), originalApi = ctx.api;
      let getRequests = 0;
      ctx.api = async (feature, request) => {
        if ((feature === 'records' || feature === 'connections') && request.action === 'get') { getRequests++; throw Error('Record not found'); }
        return originalApi(feature, request);
      };
      const root = el('main'); await mount(root, ctx);
      assert.match(root.textContent, /(?:Could not open|unavailable|cannot open)/i);
      assert.match(root.textContent, /(?:queue|search|retry|link)/i, 'error gives a recovery action');
      assert.equal(byText(root, 'Copy link'), undefined);
      assert.ok(ctx.errors.length > 0);
      assert.ok(ctx.calls.every(([, request]) => !['mark', 'overlay', 'put', 'contact', 'select-revision'].includes(request.action)));
      assert.equal(getRequests, uri === item.uri ? 1 : 0, 'bad identity is rejected before resolving a local key');
    }
  });

  test(`${view} refuses a tombstone or a different record returned for the linked key`, async () => {
    for (const patch of [{ deleted: true }, { tombstone: true }, { uri: item.uri.replace('did:sol:local', 'did:sol:other') }, { collection: 'pub.tangled.note' }, { rkey: 'different' }]) {
      const returned = { ...item, ...patch }, root = el('main');
      const ctx = linkedContext(returned, { selection: { uri: item.uri } });
      await mount(root, ctx);
      assert.match(root.textContent, /Could not open/);
      assert.equal(byText(root, 'Copy link'), undefined);
      assert.ok(ctx.calls.every(([, request]) => ['get', 'list', 'queue', 'history', 'contacts', undefined].includes(request.action)), 'record rejection does not issue a mutation');
    }
  });

  test(`${view} does not let a slow initial link replace a newer explicit selection`, async () => {
    const second = structuredClone(item); second.rkey = 'newer'; second.uri = item.uri.replace('/linked', '/newer');
    if (view === 'feeds') { second.body.title = 'Newer article'; second.body.body = 'Newer content'; }
    else second.body.imported.object.text = 'Newer source';
    let resolveLinked, requested = false;
    const pending = new Promise(resolve => { resolveLinked = resolve; });
    const updates = [], ctx = linkedContext(item, { updateRecordLink: (...args) => updates.push(args) }), api = ctx.api;
    ctx.api = (feature, request) => {
      if (request.action === 'get') { if (request.rkey === 'linked') { requested = true; return pending; } return Promise.resolve(second); }
      if (view === 'feeds' && feature === 'feeds' && request.action === 'queue') return Promise.resolve({ items: [second] });
      if (view === 'connections' && feature === 'connections' && request.action === 'list') return Promise.resolve([second]);
      return api(feature, request);
    };
    const root = el('main'), mounting = mount(root, ctx);
    await new Promise(setImmediate);
    assert.equal(requested, true, 'initial linked record is loading');
    await byText(root, view === 'feeds' ? 'Newer article' : 'Newer source').onClick();
    resolveLinked(item); await mounting;
    const reader = nodes(root).find(node => node.tagName === 'article');
    assert.ok(reader.textContent.includes(view === 'feeds' ? 'Newer article' : 'Newer source'));
    assert.ok(!reader.textContent.includes(readerText));
    assert.deepEqual(updates, [[view, second.uri]], 'new explicit selection updates the URL and a stale initial result cannot revert it');
  });
}


test('a failed source reader selection retains the previous reader and record URL', async () => {
  const second = structuredClone(linkedSource); second.rkey = 'unavailable'; second.uri = linkedSource.uri.replace('/linked', '/unavailable');
  second.body.imported.object.text = 'Unavailable source';
  const updates = [], ctx = linkedContext(linkedSource, { updateRecordLink: (...args) => updates.push(args) }), api = ctx.api;
  ctx.api = (feature, request) => {
    if (feature === 'connections' && request.action === 'list') return Promise.resolve([second]);
    if (feature === 'connections' && request.action === 'get' && request.rkey === 'unavailable') return Promise.reject(Error('Read failed. Retry this item.'));
    return api(feature, request);
  };
  const root = el('main'); await mountConnections(root, ctx);
  await byText(root, 'Unavailable source').onClick();
  assert.match(ctx.errors.at(-1), /Read failed/);
  assert.deepEqual(updates, [], 'the URL keeps identifying the successfully loaded reader');
  const reader = nodes(root).find(node => node.tagName === 'article');
  assert.match(reader.textContent, /Linked source/);
  assert.ok(!reader.textContent.includes('Unavailable source'));
});
