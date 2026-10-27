import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import { parseRoute, formatRoute, recordLink } from '../web/routes.js';

const code = (await readFile(new URL('../web/app.js', import.meta.url), 'utf8')).replace(/^import .*$/gm, '');
const credential = 'a'.repeat(64);
const uri = `tangled://did:sol:${'1'.repeat(32)}/pub.tangled.note/one`;
async function shell() {
  const listeners = new Map(), nodes = new Map(), storage = new Map([['tangled-personal.session', credential]]), mounts = [], requests = [];
  class Node {
    constructor(attrs = {}) { Object.assign(this, attrs); this.children = []; this.isConnected = false; }
    addEventListener() {}
    setAttribute(name, value) { this[name] = value; }
    removeAttribute(name) { delete this[name]; }
    replaceChildren(...children) { for (const child of this.children) child.isConnected = false; this.children = children; for (const child of children) child.isConnected = true; }
  }
  const node = id => { if (!nodes.has(id)) nodes.set(id, new Node()); return nodes.get(id); };
  const location = { pathname: '/', search: '', hash: '#notes', origin: 'http://127.0.0.1:8788', get href() { return this.origin + this.pathname + this.search + this.hash; } };
  const state = { prompts: 0, accept: false };
  const window = { addEventListener: (name, callback) => listeners.set(name, callback), confirm: () => { state.prompts++; return state.accept; } };
  const mount = name => async (root, context) => { mounts.push({ name, root, context }); };
  vm.runInNewContext(code, {
    window, location, URL, URLSearchParams, setTimeout, clearTimeout, Object, Error,
    history: { replaceState: (_, __, value) => { location.hash = new URL(value, location.href).hash; } },
    sessionStorage: { getItem: key => storage.get(key), setItem: (key, value) => storage.set(key, value), removeItem: key => storage.delete(key) },
    document: { querySelector: node, querySelectorAll: () => [] },
    parseRoute, formatRoute, recordLink,
    el: (_, attrs) => new Node(attrs), button: () => new Node(), empty: (title, description) => new Node({ title, description }),
    fetch: async (_, options) => {
      requests.push(options.headers['X-Tangled-Session']);
      const ok = options.headers['X-Tangled-Session'] === credential;
      return { ok, status: ok ? 200 : 401, headers: { get: () => 'application/json' }, json: async () => ok ? { did: 'did:sol:' + '1'.repeat(32), data: '/test' } : { error: 'Expired' } };
    },
    mountToday: mount('today'), mountNotes: mount('notes'), mountLibrary: mount('library'), mountCapture: mount('capture'),
    mountFeeds: mount('feeds'), mountConnections: mount('connections'), mountAgents: mount('agents'), mountDevices: mount('devices'), mountPublish: mount('publish'),
  });
  const flush = () => new Promise(resolve => setImmediate(resolve));
  await flush();
  return { state, mounts, location, storage, requests, nodes, listeners, flush };
}

test('a cancelled launch link retains dirty controls and the existing session', async () => {
  const app = await shell(), original = app.mounts.at(-1);
  original.context.setNavigationGuard(() => true);
  app.location.hash = '#session=' + '0'.repeat(64);
  app.listeners.get('hashchange')(); await app.flush();
  assert.equal(app.state.prompts, 1);
  assert.equal(app.storage.get('tangled-personal.session'), credential);
  assert.equal(app.requests.length, 1);
  assert.equal(original.root.isConnected, true);
  assert.equal(app.location.hash, '#notes');
});

test('late record-address updates cannot replace a pending navigation', async () => {
  const app = await shell(), previous = app.mounts.at(-1);
  app.location.hash = '#today';
  previous.context.updateRecordLink('notes', uri);
  assert.equal(app.location.hash, '#today');
  app.listeners.get('hashchange')(); await app.flush();
  assert.equal(app.mounts.at(-1).name, 'today');
  previous.context.setNavigationGuard(() => true);
  previous.context.updateRecordLink('notes', uri);
  const event = { prevented: false, preventDefault() { this.prevented = true; } };
  app.listeners.get('beforeunload')(event);
  assert.equal(event.prevented, false);
  assert.equal(app.location.hash, '#today');
});

test('current selection updates the address without remounting the editor', async () => {
  const app = await shell(), current = app.mounts.at(-1);
  current.context.updateRecordLink('notes', uri);
  assert.equal(app.location.hash, formatRoute('notes', { uri }));
  assert.equal(app.mounts.length, 1);
  assert.equal(current.context.selection.uri, uri);
  current.context.setNavigationGuard(() => true);
  const event = { prevented: false, preventDefault() { this.prevented = true; } };
  app.listeners.get('beforeunload')(event);
  assert.equal(event.prevented, true);
});
