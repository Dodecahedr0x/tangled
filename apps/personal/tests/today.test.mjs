import test from 'node:test';
import assert from 'node:assert/strict';

// A small native DOM boundary lets the real UI helpers and Today pane run in Node.
class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.events = {}; this.attributes = {}; this.dataset = {}; this.className = ''; this._text = ''; }
  append(...nodes) {
    for (const node of nodes) {
      assert.ok(!Array.isArray(node), 'Native DOM children must be flattened');
      if (node instanceof Element) node.parentElement = this;
      this.children.push(node);
    }
  }
  replaceChildren(...nodes) {
    if (document.activeElement !== this && this.contains(document.activeElement)) document.activeElement = document.body;
    for (const child of this.children) if (child instanceof Element) child.parentElement = null;
    this.children = []; this._text = ''; this.append(...nodes);
  }
  contains(node) { for (; node; node = node.parentElement) if (node === this) return true; return false; }
  get isConnected() { return this === document.body || Boolean(this.parentElement?.isConnected); }
  get disabled() { return this._disabled || false; }
  set disabled(value) { this._disabled = Boolean(value); if (value && document.activeElement === this) document.activeElement = document.body; }
  focus() { if (this.isConnected && !this.disabled) document.activeElement = this; }
  addEventListener(name, handler) { (this.events[name] ||= []).push(handler); }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  getAttribute(name) { return this.attributes[name]; }
  removeAttribute(name) { delete this.attributes[name]; }
  async fire(name) { for (const handler of this.events[name] || []) await handler({ preventDefault() {}, target: this }); }
  get textContent() { return this._text + this.children.map(node => node instanceof Element ? node.textContent : String(node)).join(''); }
  set textContent(value) { this.replaceChildren(); this._text = String(value); }
}
globalThis.Node = Element;
globalThis.document = { createElement: tag => new Element(tag), createTextNode: text => String(text) };
document.body = new Element('body'); document.activeElement = document.body;
const { mountToday } = await import('../web/today.js');
const walk = root => [root, ...root.children.filter(node => node instanceof Element).flatMap(walk)];
const findButton = (root, label) => { const node = walk(root).find(node => node.tagName === 'button' && node.textContent === label); assert.ok(node, `Missing button: ${label}`); return node; };
const section = (root, label) => { const node = walk(root).find(node => node.getAttribute('aria-label') === label); assert.ok(node, `Missing section: ${label}`); return node; };
const clone = value => structuredClone(value);
function record(collection, rkey, body) { return { uri: `tangled://did:sol:test/${collection}/${rkey}`, rkey, collection, cid: 'original', ts: 12, body }; }
const note = (key, body) => record('pub.tangled.note', key, { v: 1, text: 'A thought', state: 'note', createdAt: 1, updatedAt: 1, vendor: { retained: true }, ...body });
const capture = (key, body = {}) => record('pub.tangled.capture', key, { v: 1, kind: 'text', text: 'Saved text', read: false, starred: true, tags: ['keep'], vendor: { retained: true }, ...body });
const feed = key => ({ ...record('pub.tangled.feedItem', key, { v: 1, title: `Feed ${key}`, body: 'Article', bodyType: 'text', sourceUrl: 'https://example.org/feed', subscription: 'sub' }), reading: { v: 1, read: false, starred: true, tags: ['keep'], vendor: { retained: true } }, effectiveTags: ['keep'] });
function setup(initial = []) {
  const data = new Map(initial.map(item => [item.uri, clone(item)])), calls = [], notices = [], navigations = [];
  const ctx = { calls, notices, navigations, pending: [], failures: new Set(),
    navigate: (...args) => navigations.push(args), notify: (...args) => notices.push(args),
    run(control, task) {
      if (control.disabled) return;
      control.disabled = true;
      const promise = Promise.resolve().then(task).catch(error => notices.push([error.message, 'error'])).finally(() => { control.disabled = false; });
      ctx.pending.push(promise); return promise;
    },
    async api(feature, input) {
      calls.push({ feature, ...clone(input) });
      if (ctx.failures.has(`${feature}:${input.action}`)) throw new Error('Temporary repository error');
      const all = collection => [...data.values()].filter(item => item.collection === collection);
      if (feature === 'notes') {
        if (input.action === 'due') return clone(all('pub.tangled.note').filter(item => item.body.reminderAt != null && item.body.reminderAt <= Date.now() / 1000));
        if (input.action === 'list') return clone(all('pub.tangled.note').filter(item => !input.state || item.body.state === input.state));
        assert.equal(input.action, 'edit');
        const item = all('pub.tangled.note').find(item => item.rkey === input.rkey); assert.ok(item);
        Object.assign(item.body, clone(input.patch));
        if (item.body.reminderAt === null) delete item.body.reminderAt;
        return clone(item);
      }
      if (feature === 'capture') {
        if (input.action === 'list') return clone(all('pub.tangled.capture').filter(item => item.body.read === input.read));
        assert.ok(['mark-read', 'mark-unread'].includes(input.action));
        const item = data.get(input.uri); assert.ok(item); item.body.read = input.action === 'mark-read'; return clone(item);
      }
      assert.equal(feature, 'feeds');
      if (input.action === 'queue') return { items: clone(all('pub.tangled.feedItem').filter(item => !input.unread || !item.reading.read).slice(0, input.limit)) };
      assert.equal(input.action, 'mark');
      const item = data.get(input.item); assert.ok(item); item.reading.read = input.read; return { body: clone(item.reading) };
    },
  };
  const root = new Element('main'); document.body.replaceChildren(root);
  return { root, ctx, data };
}
async function settle(ctx) { for (let i = 0; i < 4; i++) { await Promise.all(ctx.pending); await new Promise(setImmediate); } }
async function click(root, label, ctx) { const control = findButton(root, label); control.focus(); await control.fire('click'); await settle(ctx); }

test('Today opens canonical records and quick actions using URI and editor mode routes', async () => {
  const due = note('due', { title: 'Call Ada', reminderAt: 10 }), inbox = note('inbox', { title: 'Idea', state: 'inbox' });
  const saved = capture('saved', { title: '<script>literal title</script>' }), article = feed('article');
  const { root, ctx } = setup([due, inbox, saved, article]);
  await mountToday(root, ctx);
  for (const [label, item, view] of [['Due reminders', due, 'notes'], ['Self inbox', inbox, 'notes'], ['Unread saved items', saved, 'capture'], ['Unread feed articles', article, 'feeds']]) {
    await click(section(root, label), item.body.title, ctx);
    assert.deepEqual(ctx.navigations.at(-1), [view, { uri: item.uri }]);
  }
  await click(root, 'New note', ctx); assert.deepEqual(ctx.navigations.at(-1), ['notes', { mode: 'note' }]);
  await click(root, 'Capture to self inbox', ctx); assert.deepEqual(ctx.navigations.at(-1), ['notes', { mode: 'inbox' }]);
  await click(root, 'Save an item', ctx); assert.deepEqual(ctx.navigations.at(-1), ['capture']);
  assert.deepEqual(ctx.calls.slice(0, 4), [
    { feature: 'notes', action: 'due' }, { feature: 'notes', action: 'list', state: 'inbox' },
    { feature: 'capture', action: 'list', read: false }, { feature: 'feeds', action: 'queue', unread: true, limit: 11 },
  ]);
});

test('Today clears reminders and files inbox notes with independent undo patches', async () => {
  const item = note('both', { title: 'Follow up', state: 'inbox', reminderAt: 10 });
  const { root, ctx, data } = setup([item]); await mountToday(root, ctx);
  await click(section(root, 'Due reminders'), 'Clear reminder', ctx);
  assert.equal(data.get(item.uri).body.reminderAt, undefined);
  assert.ok(!section(root, 'Due reminders').textContent.includes('Clear reminder'));
  await click(section(root, 'Self inbox'), 'File to Notes', ctx);
  assert.equal(data.get(item.uri).body.state, 'note');
  await click(root, 'Undo file to Notes', ctx);
  assert.equal(data.get(item.uri).body.state, 'inbox');
  assert.equal(data.get(item.uri).body.reminderAt, undefined);
  await click(root, 'Undo clear reminder', ctx);
  assert.equal(data.get(item.uri).body.reminderAt, 10);
  assert.deepEqual(data.get(item.uri).body.vendor, { retained: true });
  assert.deepEqual(ctx.calls.filter(call => call.action === 'edit').map(call => call.patch), [{ reminderAt: null }, { state: 'note' }, { state: 'inbox' }, { reminderAt: 10 }]);
});

test('Today marks saved and feed items read and restores only their read flags', async () => {
  const saved = capture('one', { title: 'Saved one' }), article = feed('one');
  const { root, ctx, data } = setup([saved, article]); await mountToday(root, ctx);
  await click(section(root, 'Unread saved items'), 'Mark read', ctx);
  await click(section(root, 'Unread feed articles'), 'Mark read', ctx);
  assert.equal(data.get(saved.uri).body.read, true); assert.equal(data.get(article.uri).reading.read, true);
  await click(root, 'Undo mark saved item read', ctx); await click(root, 'Undo mark feed article read', ctx);
  assert.equal(data.get(saved.uri).body.read, false); assert.equal(data.get(article.uri).reading.read, false);
  assert.deepEqual(data.get(saved.uri).body.vendor, { retained: true });
  assert.deepEqual(data.get(article.uri).reading, article.reading);
  assert.deepEqual(ctx.calls.filter(call => call.action === 'mark'), [
    { feature: 'feeds', action: 'mark', item: article.uri, read: true }, { feature: 'feeds', action: 'mark', item: article.uri, read: false },
  ]);
});

test('Today keeps other sections usable when one fails and retries that section', async () => {
  const { root, ctx } = setup([capture('saved', { title: 'Still readable' })]);
  ctx.failures.add('notes:due'); await mountToday(root, ctx);
  const failed = section(root, 'Due reminders');
  assert.match(failed.textContent, /Temporary repository error/);
  assert.ok(!failed.textContent.includes('No reminders due'));
  await click(section(root, 'Unread saved items'), 'Still readable', ctx);
  ctx.failures.clear(); const previousCalls = ctx.calls.length;
  await click(failed, 'Retry due reminders', ctx);
  assert.match(failed.textContent, /No reminders due/);
  assert.deepEqual(ctx.calls.slice(previousCalls), [{ feature: 'notes', action: 'due' }]);
});

test('Today labels truncated lists and provides routes to their complete views', async () => {
  const items = Array.from({ length: 12 }, (_, i) => [note(`n${i}`, { title: `Note ${i}`, state: 'inbox', reminderAt: 10 }), capture(`c${i}`, { title: `Capture ${i}` }), feed(`f${i}`)]).flat();
  const { root, ctx } = setup(items); await mountToday(root, ctx);
  for (const label of ['Due reminders', 'Self inbox', 'Unread saved items', 'Unread feed articles']) {
    const area = section(root, label);
    assert.equal(walk(area).filter(node => node.tagName === 'li').length, 10);
    assert.match(area.textContent, /Showing (?:the )?first 10/);
  }
  assert.ok(!root.textContent.includes('Capture 11'));
  await click(section(root, 'Unread feed articles'), 'Open Reading', ctx);
  assert.deepEqual(ctx.navigations.at(-1), ['feeds']);
});

test('Today retains rows and enables retry after a failed mutation without offering false undo', async () => {
  const item = capture('saved', { title: 'Try again' });
  const { root, ctx, data } = setup([item]); await mountToday(root, ctx);
  ctx.failures.add('capture:mark-read');
  await click(section(root, 'Unread saved items'), 'Mark read', ctx);
  assert.equal(data.get(item.uri).body.read, false);
  assert.equal(findButton(section(root, 'Unread saved items'), 'Mark read').disabled, false);
  assert.ok(root.textContent.includes('Try again'));
  assert.match(section(root, 'Unread saved items').textContent, /Temporary repository error/);
  assert.ok(!root.textContent.includes('Undo mark saved item read'));
  ctx.failures.clear(); await click(section(root, 'Unread saved items'), 'Mark read', ctx);
  assert.equal(data.get(item.uri).body.read, true);
});

test('Today keeps undo available when a write succeeds but refreshing the list fails', async () => {
  const item = capture('saved', { title: 'Read this once' });
  const { root, ctx, data } = setup([item]); await mountToday(root, ctx);
  const api = ctx.api;
  ctx.api = async (feature, input) => {
    const result = await api(feature, input);
    if (feature === 'capture' && input.action === 'mark-read') ctx.failures.add('capture:list');
    return result;
  };
  await click(section(root, 'Unread saved items'), 'Mark read', ctx);
  assert.equal(data.get(item.uri).body.read, true);
  assert.ok(!walk(section(root, 'Unread saved items')).some(node => node.tagName === 'button' && node.textContent === 'Read this once'));
  assert.match(section(root, 'Unread saved items').textContent, /Temporary repository error/);
  ctx.failures.clear(); await click(root, 'Undo mark saved item read', ctx);
  assert.equal(data.get(item.uri).body.read, false);
  findButton(section(root, 'Unread saved items'), 'Read this once');
});

test('Today retains a failed undo so it can be retried', async () => {
  const item = capture('saved', { title: 'Keep the option' });
  const { root, ctx, data } = setup([item]); await mountToday(root, ctx);
  await click(section(root, 'Unread saved items'), 'Mark read', ctx);
  ctx.failures.add('capture:mark-unread'); await click(root, 'Undo mark saved item read', ctx);
  assert.equal(data.get(item.uri).body.read, true);
  assert.equal(findButton(root, 'Undo mark saved item read').disabled, false);
  ctx.failures.clear(); await click(root, 'Undo mark saved item read', ctx);
  assert.equal(data.get(item.uri).body.read, false);
});

test('Today renders successful sections while another loads and ignores its stale response', async () => {
  const item = capture('saved', { title: 'Old saved title' });
  const { root, ctx, data } = setup([item, note('due', { title: 'Available now', reminderAt: 10 })]);
  const api = ctx.api;
  let release, first = true;
  const blocked = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, input) => {
    const result = await api(feature, input);
    if (feature === 'capture' && input.action === 'list' && first) { first = false; await blocked; }
    return result;
  };
  const mounting = mountToday(root, ctx); await new Promise(setImmediate);
  findButton(section(root, 'Due reminders'), 'Available now');
  assert.match(section(root, 'Unread saved items').textContent, /Loading/);
  data.get(item.uri).body.title = 'New saved title';
  await click(section(root, 'Unread saved items'), 'Refresh unread saved items', ctx);
  release(); await mounting;
  findButton(section(root, 'Unread saved items'), 'New saved title');
  assert.ok(!root.textContent.includes('Old saved title'));
});

test('Today puts each count with its heading in the section toolbar', async () => {
  const { root, ctx } = setup([capture('one')]); await mountToday(root, ctx);
  for (const label of ['Due reminders', 'Self inbox', 'Unread saved items', 'Unread feed articles']) {
    const area = section(root, label), heading = walk(area).find(node => node.tagName === 'h2');
    assert.equal(heading.parentElement.className, 'today-title');
    assert.equal(heading.parentElement.parentElement.className, 'toolbar');
    assert.match(heading.parentElement.textContent, /[01] items?/);
  }
});

test('Today focuses each resulting Undo and the restored record after keyboard actions', async () => {
  const cases = [
    ['Due reminders', note('due', { title: 'Due note', reminderAt: 10 }), 'Clear reminder', 'Undo clear reminder'],
    ['Self inbox', note('inbox', { title: 'Inbox note', state: 'inbox' }), 'File to Notes', 'Undo file to Notes'],
    ['Unread saved items', capture('saved', { title: 'Saved item' }), 'Mark read', 'Undo mark saved item read'],
    ['Unread feed articles', feed('article'), 'Mark read', 'Undo mark feed article read'],
  ];
  for (const [label, item, action, undo] of cases) {
    const { root, ctx } = setup([item]); await mountToday(root, ctx);
    await click(section(root, label), action, ctx);
    assert.ok(document.activeElement === findButton(root, undo), `${label}: action should focus Undo`);
    await click(root, undo, ctx);
    assert.ok(document.activeElement === findButton(section(root, label), item.body.title), `${label}: Undo should focus restored item`);
    assert.ok(document.activeElement.isConnected);
  }
});

test('Today preserves focused record and Undo controls across refreshed list renders', async () => {
  const { root, ctx } = setup([capture('one', { title: 'First saved' }), capture('two', { title: 'Second saved' })]);
  await mountToday(root, ctx);
  const area = section(root, 'Unread saved items');
  const refresh = findButton(area, 'Refresh unread saved items');
  findButton(area, 'Second saved').focus();
  await refresh.fire('click'); await settle(ctx);
  assert.ok(document.activeElement === findButton(area, 'Second saved'));
  await click(area, 'Mark read', ctx);
  const undo = findButton(area, 'Undo mark saved item read'); undo.focus();
  await refresh.fire('click'); await settle(ctx);
  assert.ok(document.activeElement === findButton(area, 'Undo mark saved item read'));
  await click(area, 'Refresh unread saved items', ctx);
  assert.ok(document.activeElement === refresh, 'Refresh must regain focus after shared run re-enables it');
});

test('Today preserves keyboard focus on a failed action and failed Undo for retry', async () => {
  const { root, ctx } = setup([capture('one', { title: 'Saved one' })]); await mountToday(root, ctx);
  const area = section(root, 'Unread saved items');
  ctx.failures.add('capture:mark-read'); await click(area, 'Mark read', ctx);
  assert.ok(document.activeElement === findButton(area, 'Mark read'));
  ctx.failures.clear(); await click(area, 'Mark read', ctx);
  ctx.failures.add('capture:mark-unread'); await click(area, 'Undo mark saved item read', ctx);
  assert.ok(document.activeElement === findButton(area, 'Undo mark saved item read'));
});

test('Today preserves another record chosen during an action instead of taking focus for Undo', async () => {
  const { root, ctx } = setup([capture('one', { title: 'First saved' }), capture('two', { title: 'Second saved' })]); await mountToday(root, ctx);
  const api = ctx.api;
  let release; const blocked = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, input) => { if (feature === 'capture' && input.action === 'mark-read') await blocked; return api(feature, input); };
  const area = section(root, 'Unread saved items'), action = findButton(area, 'Mark read');
  action.focus(); await action.fire('click'); await new Promise(setImmediate);
  findButton(area, 'Second saved').focus();
  release(); await settle(ctx);
  assert.ok(document.activeElement === findButton(area, 'Second saved'));
});

test('Today does not take focus from another connected control during action or Undo requests', async () => {
  const { root, ctx } = setup([capture('one', { title: 'Saved one' })]); await mountToday(root, ctx);
  const api = ctx.api, elsewhere = findButton(root, 'New note');
  for (const [action, request] of [['Mark read', 'mark-read'], ['Undo mark saved item read', 'mark-unread']]) {
    let release; const blocked = new Promise(resolve => { release = resolve; });
    ctx.api = async (feature, input) => { if (feature === 'capture' && input.action === request) await blocked; return api(feature, input); };
    const control = findButton(section(root, 'Unread saved items'), action);
    control.focus(); await control.fire('click'); await new Promise(setImmediate);
    elsewhere.focus(); release(); await settle(ctx);
    assert.ok(document.activeElement === elsewhere);
  }
});

test('Today focuses the next visible record when undo restores an item outside the bounded list', async () => {
  const saved = Array.from({ length: 11 }, (_, index) => capture(`c${index}`, { title: `Saved ${index}` }));
  const { root, ctx, data } = setup(saved); await mountToday(root, ctx);
  const area = section(root, 'Unread saved items'); await click(area, 'Mark read', ctx);
  // Another client can change ordering while this pane remains open.
  const first = data.get(saved[0].uri); data.delete(saved[0].uri); data.set(saved[0].uri, first);
  await click(area, 'Undo mark saved item read', ctx);
  assert.ok(document.activeElement === findButton(area, 'Saved 1'));
});


test('Today retains focus on an earlier Undo chosen while a later action completes', async () => {
  const { root, ctx } = setup([capture('one', { title: 'First saved' }), capture('two', { title: 'Second saved' })]);
  await mountToday(root, ctx);
  const area = section(root, 'Unread saved items'); await click(area, 'Mark read', ctx);
  const api = ctx.api;
  let release; const blocked = new Promise(resolve => { release = resolve; });
  ctx.api = async (feature, input) => { if (feature === 'capture' && input.action === 'mark-read') await blocked; return api(feature, input); };
  const action = findButton(area, 'Mark read'); action.focus(); await action.fire('click'); await new Promise(setImmediate);
  findButton(area, 'Undo mark saved item read').focus(); release(); await settle(ctx);
  const firstUndo = findButton(area, 'Undo mark saved item read');
  assert.ok(document.activeElement === firstUndo, 'The previous Undo remains the chosen keyboard control');
});
