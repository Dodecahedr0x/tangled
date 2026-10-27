import { el, button, empty } from './ui.js';
import { mountLibrary, mountCapture, mountNotes } from './library.js';
import { mountFeeds, mountConnections } from './reading.js';
import { mountAgents, mountDevices, mountPublish } from './manage.js';

const views = {
  library: ['Library', 'Find what you saved, wherever it came from.', mountLibrary],
  capture: ['Capture', 'Keep a link, an article, a file, or a passing thought.', mountCapture],
  notes: ['Notes', 'A quiet place for ideas, drafts, and things to follow up.', mountNotes],
  feeds: ['Reading', 'Your subscriptions and saved reading, at your pace.', mountFeeds],
  connections: ['Connections', 'Organize your existing conversations without moving the people in them.', mountConnections],
  agents: ['App access', 'Give an app or agent only the access it needs.', mountAgents],
  devices: ['Recovery', 'Keep an encrypted copy you can verify and restore.', mountDevices],
  publish: ['Publish', 'Choose what to share. Preview it before making a public copy.', mountPublish],
};
const tokenKey = 'tangled-personal.session';
let token = sessionStorage.getItem(tokenKey) || '';
function consumeLaunchLink() {
  const params = new URLSearchParams(location.hash.slice(1));
  if (!params.has('session')) return false;
  const credential = params.get('session');
  history.replaceState(null, '', `${location.pathname}${location.search}#library`);
  if (!/^[0-9a-f]{64}$/.test(credential)) return false;
  token = credential;
  sessionStorage.setItem(tokenKey, token);
  return true;
}
consumeLaunchLink();
let ready = false;
const peers = typeof BroadcastChannel === 'function' ? new BroadcastChannel('tangled-personal-tabs-v1') : null;
let noticeTimer;
const notice = document.querySelector('#notice');
const panel = document.querySelector('#view');
function notify(text, tone = 'success') {
  clearTimeout(noticeTimer);
  notice.textContent = text;
  notice.className = `notice ${tone}`;
  notice.hidden = false;
  notice.setAttribute('role', tone === 'error' ? 'alert' : 'status');
  if (tone === 'success') noticeTimer = setTimeout(() => { notice.hidden = true; }, 8000);
}
async function request(path, input, method = 'POST') {
  const response = await fetch(`/api/v1/${path}`, {
    method, credentials: 'omit', redirect: 'error',
    headers: { 'X-Tangled-Session': token, ...(method === 'POST' ? { 'Content-Type': 'application/json' } : {}) },
    ...(method === 'POST' ? { body: JSON.stringify(input) } : {}),
  });
  const contentType = response.headers.get('content-type') || '';
  const data = contentType.includes('application/json') ? await response.json() : { error: 'The local service returned an unreadable response.' };
  if (!response.ok) {
    if (response.status === 401) { const error = new Error('This session expired. Reopen the app’s launch link to reconnect.'); error.status = 401; throw error; }
    throw new Error(data.error || `The request could not finish (${response.status}). Try again.`);
  }
  return data;
}
const ctx = {
  session: {}, api: (feature, input) => request(`personal/${encodeURIComponent(feature)}`, input), request, notify,
  async run(control, task) {
    if (control?.disabled) return;
    if (control) { control.disabled = true; control.setAttribute('aria-busy', 'true'); }
    try { await task(); } catch (error) { notify(error.message || String(error), 'error'); }
    finally { if (control) { control.disabled = false; control.removeAttribute('aria-busy'); } }
  },
  download(filename, data, mime = 'application/json') {
    const content = typeof data === 'string' || data instanceof Uint8Array ? data : JSON.stringify(data, null, 2);
    const url = URL.createObjectURL(new Blob([content], { type: mime }));
    const anchor = el('a', { href: url, download: filename });
    document.body.append(anchor); anchor.click(); anchor.remove();
    setTimeout(() => URL.revokeObjectURL(url), 30000);
  },
  navigate(view) { location.hash = Object.hasOwn(views, view) ? view : 'library'; },
  async refreshSession() {
    ctx.session = await request('session', undefined, 'GET');
    document.querySelector('#library-identity').textContent = ctx.session.did;
    document.querySelector('#library-storage').textContent = ctx.session.data || 'On this computer';
    document.querySelector('#library-status').textContent = 'Connected to your local library';
    return ctx.session;
  },
};
async function route() {
  if (!ready) return;
  const name = Object.hasOwn(views, location.hash.slice(1)) ? location.hash.slice(1) : 'library';
  const [title, description, mount] = views[name];
  document.title = `${title} · Personal`;
  document.querySelector('#view-title').textContent = title;
  document.querySelector('#view-description').textContent = description;
  document.querySelectorAll('[data-view]').forEach(link => {
    if (link.dataset.view === name) link.setAttribute('aria-current', 'page');
    else link.removeAttribute('aria-current');
  });
  const root = el('section', { class: 'workspace-pane', 'aria-label': title }, el('p', { class: 'muted', text: 'Loading…' }));
  panel.replaceChildren(root);
  try { await mount(root, ctx); }
  catch (error) { root.replaceChildren(empty('Could not open this view', error.message, button('Try again', route))); notify(error.message, 'error'); }
}
async function peerSession() {
  if (!peers) return '';
  return new Promise(resolve => {
    const id = crypto.randomUUID();
    const finish = value => { clearTimeout(timer); peers.removeEventListener('message', receive); resolve(value); };
    const receive = event => {
      if (event.data?.type === 'session' && event.data.id === id && /^[0-9a-f]{64}$/.test(event.data.token)) finish(event.data.token);
    };
    const timer = setTimeout(() => finish(''), 2000);
    peers.addEventListener('message', receive);
    peers.postMessage({type:'request',id});
  });
}
peers?.addEventListener('message', async event => {
  if (!ready || !token || event.data?.type !== 'request' || typeof event.data.id !== 'string' || event.data.id.length > 64) return;
  try {
    await request('session', undefined, 'GET');
    peers.postMessage({type:'session',id:event.data.id,token});
  } catch { /* Only a currently authenticated tab may connect another same-origin tab. */ }
});
async function boot() {
  if (!token) {
    const peerToken = await peerSession();
    if (!token && peerToken) { token = peerToken; sessionStorage.setItem(tokenKey, token); }
  }
  if (!token) {
    panel.replaceChildren(empty('Open your library’s launch link', 'This tab needs the private link shown when Personal starts. The link connects you to your local library; it does not create an online account.', button('Connect to an open Personal tab', boot)));
    document.querySelector('#library-status').textContent = 'Launch link required';
    return;
  }
  try { await ctx.refreshSession(); ready = true; await route(); }
  catch (error) { ready = false; if (error.status === 401) { token = ''; sessionStorage.removeItem(tokenKey); } panel.replaceChildren(empty('Your library is not connected', error.message, button('Reconnect', boot))); notify(error.message, 'error'); }
}
document.querySelector('.skip-link').addEventListener('click', event => { event.preventDefault(); document.querySelector('#workspace').focus(); });
window.addEventListener('hashchange', () => { if (consumeLaunchLink()) { ready = false; boot(); } else route(); });
window.addEventListener('personal-error', event => notify(event.detail, 'error'));
boot();
