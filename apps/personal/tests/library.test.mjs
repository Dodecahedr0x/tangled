import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

// A deliberately small DOM contract: native replaceChildren rejects nested arrays,
// while el() accepts them. This exercises pane behavior without a browser dependency.
class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.events = {}; this.className = ''; this._text = ''; this._value = undefined; }
  append(...nodes) {
    for (const node of nodes) {
      assert.ok(!Array.isArray(node), 'Native DOM children must be flattened');
      assert.ok(node != null, 'Native DOM would render null as text');
      if (node instanceof Element) node.parentElement = this;
      this.children.push(node);
    }
  }
  replaceChildren(...nodes) { this.children = []; this._text = ''; if (this.tagName === 'select') this._value = undefined; this.append(...nodes); }
  addEventListener(name, handler) { (this.events[name] ||= []).push(handler); }
  async fire(name) { for (const handler of this.events[name] || []) await handler({ preventDefault() {}, target: this }); }
  focus() { this.focused = true; }
  get textContent() { return this._text + this.children.map(node => node instanceof Element ? node.textContent : String(node)).join(''); }
  set textContent(value) { this.children = []; this._text = String(value); }
  get value() {
    if (this.tagName === 'select' && this._value === undefined) return (this.children.find(node => node.selected) || this.children[0])?.value || '';
    return this._value || '';
  }
  set value(value) { this._value = String(value); }
  get classList() { return { add: token => { this.className += ` ${token}`; }, remove: token => { this.className = this.className.split(' ').filter(value => value !== token).join(' '); } }; }
}
function el(tag, attrs = {}, ...children) {
  const node = new Element(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (key.startsWith('on')) node.addEventListener(key.slice(2).toLowerCase(), value);
    else if (key === 'text') node.textContent = value;
    else if (key === 'class') node.className = value;
    else node[key] = value;
  }
  node.append(...children.flat(Infinity).filter(child => child != null && child !== false));
  return node;
}
function field(label, control, hint = '') { control.testLabel = label; return el('label', {}, label, control, hint); }
function button(label, onClick, kind = 'secondary') { return el('button', { type: 'button', text: label, onClick, class: kind }); }
function empty(title, description, action) { return el('section', {}, title, description, action); }
const recordTitle = record => record.body?.title || record.body?.name || record.body?.text?.slice(0, 80) || record.collection || 'Untitled';
const reveals = [];
globalThis.__libraryTestUi = { el, field, button, empty, recordTitle, timeLabel: value => new Date(value).toISOString(), revealSelection: node => reveals.push(node) };
const source = (await readFile(new URL('../web/library.js', import.meta.url), 'utf8')).replace(/import \{[^\n]+\} from '\.\/ui\.js';/, 'const { el, field, button, empty, recordTitle, timeLabel, revealSelection } = globalThis.__libraryTestUi;');
const { mountLibrary, mountCapture, mountNotes } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const walk = root => [root, ...root.children.filter(node => node instanceof Element).flatMap(walk)];
const control = (root, label) => { const result = walk(root).find(node => node.testLabel === label); assert.ok(result, `Missing field: ${label}`); return result; };
const findButton = (root, label) => { const result = walk(root).find(node => node.tagName === 'button' && node.textContent === label); assert.ok(result, `Missing button: ${label}`); return result; };
async function settle(ctx) { for (let i = 0; i < 5; i++) { await Promise.all(ctx.pending); await new Promise(setImmediate); } }
async function click(root, label, ctx) { await findButton(root, label).fire('click'); await settle(ctx); }
async function submitForm(root, label, ctx) { let form = findButton(root, label); while (form.tagName !== 'form') form = form.parentElement; await form.fire('submit'); await settle(ctx); }
const clone = value => structuredClone(value);
function setup(initial = []) {
  let sequence = initial.length;
  const data = new Map(initial.map(record => [record.uri, clone(record)]));
  const calls = [], downloads = [], notices = [];
  const ctx = { calls, downloads, notices, pending: [], failNext: null, session: { did: 'did:sol:test' }, selection: {},
    guard: () => false, discard: true, confirmations: 0, copied: [],
    setNavigationGuard(check) { ctx.guard = check; },
    confirmDiscard() { if (!ctx.guard()) return true; ctx.confirmations++; return ctx.discard; },
    copyRecordLink(view, uri) { ctx.copied.push({ view, uri }); },
    notify: (...args) => notices.push(args),
    download: (...args) => downloads.push(args),
    run(control, task) {
      control.disabled = true;
      const promise = Promise.resolve().then(task).catch(error => notices.push([error.message, 'error'])).finally(() => { control.disabled = false; });
      ctx.pending.push(promise); return promise;
    },
    async api(feature, payload) {
      calls.push({ feature, ...clone(payload) });
      if (ctx.failNext && ctx.failNext === `${feature}:${payload.action}`) { ctx.failNext = null; throw new Error('Temporary storage error'); }
      const all = collection => [...data.values()].filter(record => !collection || record.collection === collection);
      const find = () => {
        const record = payload.uri ? data.get(payload.uri) : all().find(record => record.collection === payload.collection && record.rkey === payload.rkey);
        assert.ok(record, `Missing ${payload.uri || payload.rkey}`); return clone(record);
      };
      const save = (collection, key, body) => {
        const rkey = key || `key-${++sequence}`, uri = `tangled://did:sol:test/${collection}/${rkey}`;
        const record = { uri, rkey, collection, cid: `revision-${++sequence}`, body: clone(body) };
        data.set(uri, record); return clone(record);
      };
      if (feature === 'records') {
        if (payload.action === 'list') return { records: clone(all(payload.collection)) };
        if (payload.action === 'get') return find();
      }
      if (feature === 'memory') {
        if (payload.action === 'list-searches') return { records: clone(all('pub.tangled.savedSearch')) };
        if (payload.action === 'list-collections') return { records: clone(all('pub.tangled.collection')) };
        if (payload.action === 'search') {
          let found = all().filter(record => !['pub.tangled.savedSearch', 'pub.tangled.collection'].includes(record.collection));
          if (payload.collection) found = found.filter(record => record.collection === payload.collection);
          if (payload.group) { const group = all('pub.tangled.collection').find(record => record.rkey === payload.group); found = found.filter(record => group.body.items.some(item => item.uri === record.uri)); }
          if (payload.query) found = found.filter(record => `${recordTitle(record)} ${record.body.text || ''}`.toLowerCase().includes(payload.query.toLowerCase()));
          return { total: found.length, results: clone(found.slice(payload.offset, payload.offset + payload.limit).map(record => ({ ...record, title: recordTitle(record), snippet: record.body.text || '' }))) };
        }
        const collection = { 'save-search': 'pub.tangled.savedSearch', 'put-collection': 'pub.tangled.collection', annotate: 'pub.tangled.annotation', 'delete-search': 'pub.tangled.savedSearch', 'delete-collection': 'pub.tangled.collection', 'delete-annotation': 'pub.tangled.annotation' }[payload.action];
        if (collection) {
          const existing = all(collection).find(record => record.rkey === payload.rkey);
          if (payload.action.startsWith('delete')) { data.delete(existing.uri); return existing; }
          const body = { ...(existing?.body || {}), ...payload }; delete body.action; delete body.rkey;
          if (collection === 'pub.tangled.collection') body.items ||= [];
          return save(collection, payload.rkey, body);
        }
      }
      if (feature === 'capture') {
        if (payload.action === 'list') return clone(all('pub.tangled.capture').filter(record => (payload.read === undefined || record.body.read === payload.read) && (payload.starred === undefined || record.body.starred === payload.starred) && (!payload.tag || record.body.tags.includes(payload.tag))));
        if (payload.action === 'save') return save('pub.tangled.capture', undefined, { ...payload, read: false, starred: false, archivedText: payload.text });
        const record = find();
        if (payload.action === 'get') return record;
        if (payload.action === 'read') return { text: record.body.archivedText || record.body.text || 'Download original', mime: 'text/plain' };
        if (payload.action === 'download') return { bytes: record.body.bytes || [72, 105, 0, 255], mime: record.body.mime || 'text/plain' };
        if (payload.action === 'delete') { data.delete(record.uri); return record; }
        const patch = payload.action === 'update' ? payload.patch : payload.action === 'star' ? { starred: payload.starred } : { read: payload.action === 'mark-read' };
        if (patch.text !== undefined) patch.archivedText = patch.text;
        return save(record.collection, record.rkey, { ...record.body, ...patch });
      }
      if (feature === 'notes') {
        if (['list', 'due'].includes(payload.action)) return clone(all('pub.tangled.note').filter(record => (!payload.state || record.body.state === payload.state) && (payload.action !== 'due' || record.body.reminderAt <= Date.now() / 1000)));
        if (['create', 'capture', 'convert'].includes(payload.action)) {
          assert.notEqual(payload.body.title, null, 'Create must omit an unset title');
          assert.notEqual(payload.body.reminderAt, null, 'Create must omit an unset reminder');
          const body = { ...payload.body };
          if (payload.action === 'capture') body.state = 'inbox';
          if (payload.action === 'convert') { const original = all().find(record => record.collection === payload.source.collection && record.rkey === payload.source.rkey); body.provenance = { uri: original.uri, cid: original.cid }; }
          return save('pub.tangled.note', undefined, body);
        }
        const record = all('pub.tangled.note').find(record => record.rkey === payload.rkey); assert.ok(record);
        if (payload.action === 'get') return clone(record);
        if (payload.action === 'delete') { data.delete(record.uri); return record; }
        if (payload.action === 'attachment') return { name: record.body.attachments[payload.index].name, mime: 'application/octet-stream', bytes: [0, 255, 1] };
        let patch = payload.patch || {};
        if (payload.action === 'append') patch = { text: `${record.body.text}\n${payload.text}` };
        if (payload.action === 'attach') patch = { attachments: [...(record.body.attachments || []), { name: payload.name, mime: payload.mime, cid: 'file-cid' }] };
        const body = { ...record.body, ...patch };
        for (const name of ['title', 'reminderAt', 'attachments']) if (body[name] === null) delete body[name];
        return save(record.collection, record.rkey, body);
      }
      assert.fail(`Unexpected action ${feature}:${payload.action}`);
    }
  };
  return { root: el('main'), ctx, data };
}
function record(collection, rkey, body) { return { collection, rkey, uri: `tangled://did:sol:test/${collection}/${rkey}`, cid: 'original-cid', body }; }

test('Library opens records, saves filters, manages collection membership and annotations', async () => {
  const savedNote = record('pub.tangled.note', 'one', { title: 'Meeting', text: '<b>Plain text</b>', state: 'note', vendor: { retained: true } });
  const { root, ctx, data } = setup([savedNote]);
  await mountLibrary(root, ctx);
  control(root, 'Collection name').value = 'Work';
  await submitForm(root, 'Save collection', ctx);
  assert.ok(root.textContent.includes('Work'));
  await click(root, 'Meeting', ctx);
  assert.ok(walk(root).some(node => node.className === 'reader-body' && node.textContent === '<b>Plain text</b>'));
  await click(root, 'Add to collection', ctx);
  const group = [...data.values()].find(record => record.collection === 'pub.tangled.collection');
  assert.deepEqual(group.body.items, [{ uri: savedNote.uri, cid: savedNote.cid }]);
  control(root, 'Annotation').value = 'Ask about <script>literal content</script>';
  control(root, 'Annotation tags').value = 'Work, budget, Work';
  await submitForm(root, 'Save annotation', ctx);
  let annotation = [...data.values()].find(record => record.collection === 'pub.tangled.annotation');
  assert.deepEqual(annotation.body.tags, ['Work', 'budget']);
  await click(root, 'Edit annotation', ctx);
  control(root, 'Annotation').value = 'Revised question';
  await submitForm(root, 'Save annotation', ctx);
  annotation = [...data.values()].find(record => record.collection === 'pub.tangled.annotation');
  assert.equal(annotation.body.text, 'Revised question');
  control(root, 'Search your library').value = 'Meeting';
  control(root, 'Search name').value = 'Meetings';
  await submitForm(root, 'Save search', ctx);
  assert.equal([...data.values()].find(record => record.collection === 'pub.tangled.savedSearch').body.query, 'Meeting');
  await click(root, 'Run saved search', ctx);
  await click(root, 'Delete annotation', ctx);
  assert.equal([...data.values()].filter(record => record.collection === 'pub.tangled.annotation').length, 0);
  await click(root, 'Remove', ctx);
  assert.deepEqual([...data.values()].find(record => record.collection === 'pub.tangled.collection').body.items, []);
  await click(root, 'Delete collection', ctx);
  assert.ok(data.has(savedNote.uri));
  await click(root, 'Delete saved search', ctx);
});

test('Capture preserves failed forms and reads article blobs, then edits and downloads original bytes', async () => {
  const { root, ctx, data } = setup();
  await mountCapture(root, ctx);
  control(root, 'What are you saving?').value = 'article'; await control(root, 'What are you saving?').fire('change');
  control(root, 'Title (optional)').value = 'Archive';
  control(root, 'Original URL').value = 'https://example.org/article';
  control(root, 'Saved text').value = 'Article body <img src=x onerror=alert(1)>';
  ctx.failNext = 'capture:save';
  await submitForm(root, 'Save item', ctx);
  assert.equal(control(root, 'Title (optional)').value, 'Archive');
  assert.match(root.textContent, /Temporary storage error/);
  assert.equal(findButton(root, 'Save item').disabled, false);
  await submitForm(root, 'Save item', ctx);
  assert.ok(ctx.calls.some(call => call.feature === 'capture' && call.action === 'read'));
  assert.ok(walk(root).some(node => node.className === 'reader-body' && node.textContent.includes('<img')));
  control(root, 'Title').value = 'Archive revised';
  control(root, 'Tags').value = 'reading'; // First Tags is the capture form; edit the second below.
  walk(root).filter(node => node.testLabel === 'Tags')[1].value = 'reading, later';
  walk(root).filter(node => node.testLabel === 'Saved text')[1].value = 'Updated article';
  await submitForm(root, 'Save changes', ctx);
  assert.equal([...data.values()][0].body.text, 'Updated article');
  await click(root, 'Mark read', ctx); await click(root, 'Star item', ctx);
  assert.equal([...data.values()][0].body.read, true); assert.equal([...data.values()][0].body.starred, true);
  await click(root, 'Download original', ctx);
  assert.deepEqual([...ctx.downloads[0][1]], [72, 105, 0, 255]);
  await click(root, 'Delete saved item', ctx);
  assert.equal(data.size, 0);
});

test('Capture file picker supplies exact bytes and validates screenshot MIME', async () => {
  const { root, ctx } = setup(); await mountCapture(root, ctx);
  const kind = control(root, 'What are you saving?'), file = control(root, 'Choose a file');
  kind.value = 'screenshot'; await kind.fire('change');
  file.files = [{ name: 'wrong.txt', type: 'text/plain', size: 2, arrayBuffer: async () => Uint8Array.from([0, 255]).buffer }];
  await submitForm(root, 'Save item', ctx);
  assert.ok(!ctx.calls.some(call => call.action === 'save'));
  file.files = [{ name: 'screen.png', type: 'image/png', size: 3, arrayBuffer: async () => Uint8Array.from([0, 128, 255]).buffer }];
  await submitForm(root, 'Save item', ctx);
  const request = ctx.calls.find(call => call.action === 'save');
  assert.deepEqual(request.bytes, [0, 128, 255]); assert.equal(request.title, 'screen.png');
});

test('Notes create untitled drafts, keep failed values and preserve unknown fields through editing', async () => {
  const { root, ctx, data } = setup(); await mountNotes(root, ctx);
  await click(root, 'New draft', ctx);
  ctx.failNext = 'notes:create';
  control(root, 'Text').value = 'Draft retained after an error';
  await submitForm(root, 'Create note', ctx);
  assert.equal(control(root, 'Text').value, 'Draft retained after an error');
  await submitForm(root, 'Create note', ctx);
  const created = [...data.values()][0];
  assert.equal(created.body.state, 'draft'); assert.equal(created.body.title, undefined); assert.equal(created.body.reminderAt, undefined);
  data.get(created.uri).body.vendor = { retained: true };
  control(root, 'Title (optional)').value = 'Updated';
  control(root, 'Keep as').value = 'note';
  control(root, 'Reminder').value = '2020-01-01T12:00';
  await submitForm(root, 'Save note', ctx);
  assert.deepEqual(data.get(created.uri).body.vendor, { retained: true });
  await click(root, 'Clear reminder', ctx); await submitForm(root, 'Save note', ctx);
  assert.equal(data.get(created.uri).body.reminderAt, undefined);
  control(root, 'Show').value = 'due'; await submitForm(root, 'Apply filters', ctx);
  assert.ok(ctx.calls.some(call => call.feature === 'notes' && call.action === 'due'));
});

test('Notes attachment actions retain pending edits, download bytes and append text', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Original', text: 'First line', state: 'note' });
  const { root, ctx, data } = setup([saved]); await mountNotes(root, ctx);
  await click(root, 'Original', ctx);
  control(root, 'Text').value = 'Unsaved edit'; control(root, 'Title (optional)').value = 'Pending title';
  control(root, 'File to attach').files = [{ name: 'bytes.bin', type: 'application/octet-stream', size: 3, arrayBuffer: async () => Uint8Array.from([0, 255, 1]).buffer }];
  await submitForm(root, 'Attach file', ctx);
  assert.equal(control(root, 'Text').value, 'Unsaved edit'); assert.equal(control(root, 'Title (optional)').value, 'Pending title');
  assert.equal(data.get(saved.uri).body.text, 'First line');
  await click(root, 'Download', ctx); assert.deepEqual([...ctx.downloads[0][1]], [0, 255, 1]);
  control(root, 'Text to append').value = 'Appended';
  await submitForm(root, 'Append to note', ctx);
  assert.equal(control(root, 'Text').value, 'Unsaved edit\nAppended');
  await submitForm(root, 'Save note', ctx);
  assert.equal(data.get(saved.uri).body.text, 'Unsaved edit\nAppended');
  await click(root, 'Remove attachment', ctx); assert.equal(data.get(saved.uri).body.attachments.length, 0);
});

test('Notes self inbox and source conversion use supplied text and keep provenance', async () => {
  const article = record('pub.tangled.capture', 'article', { title: 'Saved article', kind: 'article', archivedText: 'Blob article body' });
  const { root, ctx, data } = setup([article]); await mountNotes(root, ctx);
  await click(root, 'Capture to self inbox', ctx); control(root, 'Text').value = 'Inbox idea';
  await submitForm(root, 'Create note', ctx);
  assert.equal([...data.values()].find(record => record.collection === 'pub.tangled.note').body.state, 'inbox');
  await click(root, 'Create from saved item', ctx);
  const picker = control(root, 'Source item'); picker.value = article.uri; await picker.fire('change');
  assert.equal(control(root, 'Note text').value, 'Blob article body');
  control(root, 'Note title').value = '';
  await submitForm(root, 'Create note from source', ctx);
  const converted = [...data.values()].find(record => record.body.provenance);
  assert.equal(converted.body.text, 'Blob article body'); assert.deepEqual(converted.body.provenance, { uri: article.uri, cid: article.cid });
  assert.ok(data.has(article.uri));
  await click(root, 'Delete note', ctx); assert.ok(!data.has(converted.uri));
});

test('A capture text decoding error keeps original download and metadata actions available', async () => {
  const file = record('pub.tangled.capture', 'binary-text', { title: 'Non-UTF8 file', kind: 'file', mime: 'text/plain', tags: [], bytes: [255, 0], read: false });
  const { root, ctx } = setup([file]); await mountCapture(root, ctx);
  ctx.failNext = 'capture:read';
  await click(root, 'Non-UTF8 file', ctx);
  assert.match(root.textContent, /Saved text could not be displayed/);
  await click(root, 'Download original', ctx);
  assert.deepEqual([...ctx.downloads[0][1]], [255, 0]);
  assert.ok(findButton(root, 'Mark read'));
  assert.ok(findButton(root, 'Delete saved item'));
});

test('Library reveals only a completed explicit list selection, never organizers or refreshed results', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Selected source', text: 'Source text', state: 'note' });
  const { root, ctx } = setup([saved]);
  const start = reveals.length;
  await mountLibrary(root, ctx);
  assert.equal(reveals.length, start, 'Mount must keep its position');
  await click(root, 'Selected source', ctx);
  assert.equal(reveals.length, start + 1);
  assert.equal(reveals.at(-1)['aria-label'], 'Selected library record');
  control(root, 'Annotation').value = 'An annotation';
  await submitForm(root, 'Save annotation', ctx);
  await submitForm(root, 'Search', ctx);
  assert.equal(reveals.length, start + 1, 'Annotation save and result refresh must keep their position');
});

test('Capture reveals list selections but not updates, filter refreshes or newly saved items', async () => {
  const saved = record('pub.tangled.capture', 'one', { title: 'Saved text', text: 'Content', kind: 'text', tags: [], read: false });
  const { root, ctx } = setup([saved]);
  const start = reveals.length;
  await mountCapture(root, ctx);
  assert.equal(reveals.length, start);
  await click(root, 'Saved text', ctx);
  assert.equal(reveals.length, start + 1);
  assert.equal(reveals.at(-1)['aria-label'], 'Saved item reader');
  await click(root, 'Mark read', ctx);
  await submitForm(root, 'Apply filters', ctx);
  control(root, 'Title').value = 'Edited';
  await submitForm(root, 'Save changes', ctx);
  control(root, 'What are you saving?').value = 'text'; await control(root, 'What are you saving?').fire('change');
  control(root, 'Title (optional)').value = 'Another saved item';
  control(root, 'Saved text').value = 'New supplied content';
  await submitForm(root, 'Save item', ctx);
  assert.equal(reveals.length, start + 1, 'Save/update/filter flows must keep their position');
});

test('Notes reveals only list selection and keeps editor actions in place', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Existing note', text: 'Content', state: 'note' });
  const { root, ctx } = setup([saved]);
  const start = reveals.length;
  await mountNotes(root, ctx);
  assert.equal(reveals.length, start);
  await click(root, 'Existing note', ctx);
  assert.equal(reveals.length, start + 1);
  assert.equal(reveals.at(-1)['aria-label'], 'Note editor');
  control(root, 'Text').value = 'Edited';
  await submitForm(root, 'Save note', ctx);
  await submitForm(root, 'Apply filters', ctx);
  await click(root, 'New draft', ctx);
  await submitForm(root, 'Create note', ctx);
  assert.equal(reveals.length, start + 1, 'Save/filter/new-draft flows must keep their position');
});

test('Library and Capture ignore stale asynchronous selections when revealing readers', async () => {
  for (const [mount, collection, feature] of [[mountLibrary, 'pub.tangled.note', 'records'], [mountCapture, 'pub.tangled.capture', 'capture']]) {
    const first = record(collection, 'first', { title: 'First selection', text: 'First body', kind: 'text', state: 'note', tags: [] });
    const second = record(collection, 'second', { title: 'Second selection', text: 'Second body', kind: 'text', state: 'note', tags: [] });
    const { root, ctx } = setup([first, second]);
    const start = reveals.length;
    await mount(root, ctx);
    const api = ctx.api;
    let resolveFirst;
    const delay = new Promise(resolve => { resolveFirst = resolve; });
    ctx.api = async (target, payload) => {
      if (target === feature && payload.action === 'get' && (payload.rkey === 'first' || payload.uri === first.uri)) await delay;
      return api(target, payload);
    };
    await click(root, 'First selection', ctx);
    assert.equal(reveals.length, start, 'Pending selections do not reveal an incomplete reader');
    await click(root, 'Second selection', ctx);
    assert.equal(reveals.length, start + 1);
    resolveFirst(); await settle(ctx);
    assert.equal(reveals.length, start + 1, 'Late responses cannot reveal a stale selection');
    assert.ok(reveals.at(-1).textContent.includes('Second body'));
  }
});


test('Notes guard every saved field, cancel pane replacements, preserve filters, and clear only after successful save', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Original', text: 'Body', state: 'note' });
  const other = record('pub.tangled.note', 'two', { title: 'Other', text: 'Other body', state: 'note' });
  const { root, ctx, data } = setup([saved, other]); await mountNotes(root, ctx);
  await click(root, 'Original', ctx);
  assert.equal(ctx.guard(), false);
  for (const [label, value] of [['Title (optional)', 'Changed'], ['Text', 'Changed body'], ['Keep as', 'draft'], ['Reminder', '2030-01-01T12:00']]) {
    const item = control(root, label), previous = item.value; item.value = value;
    assert.equal(ctx.guard(), true, label); item.value = previous;
    assert.equal(ctx.guard(), false, `Reverting ${label} restores clean state`);
  }
  control(root, 'Text').value = 'Unsaved'; ctx.discard = false;
  for (const label of ['New note', 'New draft', 'Capture to self inbox', 'Other', 'Create from saved item', 'Delete note']) {
    await click(root, label, ctx);
    assert.equal(control(root, 'Text').value, 'Unsaved', label);
    assert.ok(data.has(saved.uri));
  }
  control(root, 'Show').value = 'draft'; await submitForm(root, 'Apply filters', ctx);
  assert.equal(control(root, 'Text').value, 'Unsaved'); assert.equal(ctx.guard(), true);
  ctx.failNext = 'notes:edit'; await submitForm(root, 'Save note', ctx);
  assert.equal(ctx.guard(), true); assert.equal(control(root, 'Text').value, 'Unsaved');
  await submitForm(root, 'Save note', ctx);
  assert.equal(ctx.guard(), false); assert.equal(data.get(saved.uri).body.text, 'Unsaved');
  control(root, 'Text').value = 'Discarded'; ctx.discard = true; await click(root, 'New draft', ctx);
  assert.equal(control(root, 'Text').value, ''); assert.equal(control(root, 'Keep as').value, 'draft'); assert.equal(ctx.guard(), false);
});

test('Notes preserve typing during an asynchronous save and attachment updates keep the saved baseline', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Original', text: 'Body', state: 'note', vendor: { keep: true } });
  const { root, ctx, data } = setup([saved]); await mountNotes(root, ctx); await click(root, 'Original', ctx);
  const api = ctx.api; let release; const gate = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, payload) => { if (feature === 'notes' && payload.action === 'edit') await gate; return api(feature, payload); };
  control(root, 'Text').value = 'Sent';
  let form = findButton(root, 'Save note'); while (form.tagName !== 'form') form = form.parentElement;
  await form.fire('submit'); await new Promise(setImmediate);
  assert.equal(ctx.guard(), true, 'Pending save protects navigation');
  control(root, 'Text').value = 'Typed while saving'; release(); await settle(ctx);
  assert.equal(data.get(saved.uri).body.text, 'Sent');
  assert.equal(control(root, 'Text').value, 'Typed while saving'); assert.equal(ctx.guard(), true);
  control(root, 'File to attach').files = [{ name: 'test.bin', type: 'application/octet-stream', size: 1, arrayBuffer: async () => Uint8Array.of(1).buffer }];
  await submitForm(root, 'Attach file', ctx);
  assert.equal(control(root, 'Text').value, 'Typed while saving'); assert.equal(ctx.guard(), true);
  assert.deepEqual(data.get(saved.uri).body.vendor, { keep: true });
  await submitForm(root, 'Save note', ctx); assert.equal(ctx.guard(), false);
});

test('A late note save cannot replace a newer selected editor after an approved discard', async () => {
  const one = record('pub.tangled.note', 'one', { title: 'One', text: 'First', state: 'note' });
  const two = record('pub.tangled.note', 'two', { title: 'Two', text: 'Second', state: 'note' });
  const { root, ctx } = setup([one, two]); await mountNotes(root, ctx); await click(root, 'One', ctx);
  const api = ctx.api; let release; const gate = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, payload) => { if (feature === 'notes' && payload.action === 'edit') await gate; return api(feature, payload); };
  control(root, 'Text').value = 'Saved first';
  let form = findButton(root, 'Save note'); while (form.tagName !== 'form') form = form.parentElement;
  await form.fire('submit'); await new Promise(setImmediate);
  await findButton(root, 'Two').fire('click');
  assert.equal(control(root, 'Text').value, 'Second'); release(); await settle(ctx);
  assert.equal(control(root, 'Text').value, 'Second');
});

test('Library, Capture and Notes open initial record links outside the list and copy canonical links', async () => {
  for (const [mount, view, collection] of [[mountLibrary, 'library', 'vendor.record'], [mountCapture, 'capture', 'pub.tangled.capture'], [mountNotes, 'notes', 'pub.tangled.note']]) {
    const saved = record(collection, 'hidden', { title: 'Linked record', text: 'Linked body', state: 'draft', kind: 'text', tags: [] });
    const { root, ctx } = setup([saved]); ctx.selection = { uri: saved.uri };
    const api = ctx.api;
    ctx.api = async (feature, payload) => {
      if (payload.action === 'search') return { results: [], total: 0 };
      if (payload.action === 'list' && ['capture', 'notes'].includes(feature)) return [];
      return api(feature, payload);
    };
    const start = reveals.length; await mount(root, ctx);
    assert.ok(root.textContent.includes('Linked body') || control(root, 'Text').value === 'Linked body');
    assert.equal(reveals.length, start + 1, view);
    await click(root, 'Copy link', ctx); assert.deepEqual(ctx.copied, [{ view, uri: saved.uri }]);
  }
});

test('Record links reject foreign, missing and incorrect-collection records without mutation', async () => {
  for (const [mount, collection] of [[mountLibrary, 'vendor.record'], [mountCapture, 'pub.tangled.capture'], [mountNotes, 'pub.tangled.note']]) {
    const saved = record(collection, 'one', { title: 'Local record', text: 'Body', kind: 'text', state: 'note', tags: [] });
    for (const uri of [saved.uri.replace('did:sol:test', 'did:sol:foreign'), saved.uri.replace('/one', '/missing'), 'tangled://did:sol:test/wrong.collection/one']) {
      const { root, ctx, data } = setup([saved]); ctx.selection = { uri }; await mount(root, ctx);
      assert.match(root.textContent, /unavailable|another library|not available|Could not|cannot|does not belong/i);
      assert.equal(walk(root).some(node => node.tagName === 'button' && node.textContent === 'Copy link'), false);
      assert.equal(data.size, 1);
      assert.ok(!ctx.calls.some(call => ['edit', 'delete', 'create', 'convert'].includes(call.action)));
    }
  }
});

test('Notes initial quick modes create clean editors without persisting drafts', async () => {
  for (const mode of ['note', 'draft', 'inbox']) {
    const { root, ctx, data } = setup(); ctx.selection = { mode }; await mountNotes(root, ctx);
    assert.equal(control(root, 'Keep as').value, mode); assert.equal(ctx.guard(), false); assert.equal(data.size, 0);
  }
});

test('A failed note deletion keeps a usable editor and its dirty guard', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Original', text: 'Body', state: 'note' });
  const { root, ctx, data } = setup([saved]); await mountNotes(root, ctx); await click(root, 'Original', ctx);
  control(root, 'Text').value = 'Keep me'; ctx.failNext = 'notes:delete'; await click(root, 'Delete note', ctx);
  assert.equal(control(root, 'Text').value, 'Keep me'); assert.equal(ctx.guard(), true);
  await submitForm(root, 'Save note', ctx);
  assert.equal(data.get(saved.uri).body.text, 'Keep me'); assert.equal(ctx.guard(), false);
});

test('A saved new note remains selected and clean when the list refresh fails', async () => {
  const { root, ctx, data } = setup(); await mountNotes(root, ctx);
  control(root, 'Text').value = 'Created once'; ctx.failNext = 'notes:list'; await submitForm(root, 'Create note', ctx);
  assert.equal(data.size, 1); assert.equal(ctx.guard(), false);
  control(root, 'Text').value = 'Edited'; await submitForm(root, 'Save note', ctx);
  assert.equal(data.size, 1); assert.equal([...data.values()][0].body.text, 'Edited');
});

test('Conversion edits use the navigation guard and survive a save request and failure', async () => {
  const saved = record('vendor.source', 'one', { title: 'Source', text: 'Source body' });
  const { root, ctx, data } = setup([saved]); await mountNotes(root, ctx); await click(root, 'Create from saved item', ctx);
  const picker = control(root, 'Source item'); picker.value = saved.uri; await picker.fire('change');
  control(root, 'Note text').value = 'Pending converted text'; assert.equal(ctx.guard(), true);
  ctx.discard = false; await click(root, 'Cancel conversion', ctx); await click(root, 'New draft', ctx);
  assert.equal(control(root, 'Note text').value, 'Pending converted text');
  ctx.failNext = 'notes:convert'; await submitForm(root, 'Create note from source', ctx);
  assert.equal(ctx.guard(), true); assert.equal(control(root, 'Note text').value, 'Pending converted text');
  const api = ctx.api; let release; const gate = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, payload) => { if (feature === 'notes' && payload.action === 'convert') await gate; return api(feature, payload); };
  let form = findButton(root, 'Create note from source'); while (form.tagName !== 'form') form = form.parentElement;
  await form.fire('submit'); await new Promise(setImmediate);
  control(root, 'Note text').value = 'Typed during conversion'; release(); await settle(ctx);
  assert.equal(control(root, 'Text').value, 'Typed during conversion'); assert.equal(ctx.guard(), true);
  const created = [...data.values()].find(item => item.collection === 'pub.tangled.note');
  assert.equal(created.body.text, 'Pending converted text');
});

test('A late note completion cannot re-register the guard after leaving the view', async () => {
  const saved = record('pub.tangled.note', 'one', { title: 'Original', text: 'Body', state: 'note' });
  const { root, ctx } = setup([saved]); await mountNotes(root, ctx); await click(root, 'Original', ctx);
  const api = ctx.api; let release; const gate = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, payload) => { if (feature === 'notes' && payload.action === 'edit') await gate; return api(feature, payload); };
  control(root, 'Text').value = 'Save before leaving';
  let form = findButton(root, 'Save note'); while (form.tagName !== 'form') form = form.parentElement;
  await form.fire('submit'); await new Promise(setImmediate);
  const editor = walk(root).find(node => node['aria-label'] === 'Note editor'); editor.isConnected = false;
  const nextGuard = () => false; ctx.setNavigationGuard(nextGuard);
  release(); await settle(ctx); assert.equal(ctx.guard, nextGuard);
});

test('Conversion uses one editor and a late source read preserves typed text', async () => {
  const saved = record('pub.tangled.capture', 'one', { title: 'Source', archivedText: 'Loaded body', kind: 'article' });
  const { root, ctx } = setup([saved]); await mountNotes(root, ctx); await click(root, 'Create from saved item', ctx);
  assert.equal(walk(root).some(node => node.testLabel === 'Text'), false, 'Only conversion controls are editable');
  const api = ctx.api; let release; const gate = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, payload) => { if (feature === 'capture' && payload.action === 'read') await gate; return api(feature, payload); };
  const picker = control(root, 'Source item'); picker.value = saved.uri; const changing = picker.fire('change');
  await new Promise(setImmediate); control(root, 'Note text').value = 'Typed before source loaded'; release(); await changing;
  assert.equal(control(root, 'Note text').value, 'Typed before source loaded');
  await click(root, 'Cancel conversion', ctx); assert.equal(control(root, 'Text').value, ''); assert.equal(ctx.guard(), false);
});

test('Initial links verify the exact record returned before offering record actions', async () => {
  for (const [mount, feature, collection] of [[mountLibrary, 'records', 'vendor.record'], [mountCapture, 'capture', 'pub.tangled.capture'], [mountNotes, 'records', 'pub.tangled.note']]) {
    const saved = record(collection, 'one', { title: 'Wrong record', text: 'Body', kind: 'text', tags: [], state: 'note' });
    const { root, ctx } = setup([saved]); ctx.selection = { uri: saved.uri.replace('/one', '/another') };
    const api = ctx.api;
    ctx.api = async (target, payload) => target === feature && payload.action === 'get' ? clone(saved) : api(target, payload);
    await mount(root, ctx); assert.match(root.textContent, /unavailable/i);
    assert.equal(walk(root).some(node => node.tagName === 'button' && node.textContent === 'Copy link'), false);
  }
});
