import { randomBytes } from 'node:crypto';

const REQUEST_TIMEOUT_MS = 10_000;
const MAX_RESPONSE_BYTES = 1024 * 1024;

function localBase(baseUrl) {
  let url;
  try { url = new URL(baseUrl); } catch { /* Report a fixed message below. */ }
  if (!url || url.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(url.hostname)
    || url.username || url.password || url.pathname !== '/' || url.search || url.hash) {
    throw new Error('Demo base URL must be credential-free numeric loopback HTTP with no path, query or fragment.');
  }
  return url;
}

async function request(base, path, options, signal, label) {
  const deadline = AbortSignal.timeout(REQUEST_TIMEOUT_MS);
  const combined = signal ? AbortSignal.any([signal, deadline]) : deadline;
  try {
    const response = await fetch(new URL(path, base), {
      ...options, redirect: 'manual', signal: combined,
    });
    const reader = response.body?.getReader();
    const chunks = [];
    let length = 0;
    if (reader) {
      try {
        for (;;) {
          const { done, value } = await reader.read();
          if (done) break;
          length += value.byteLength;
          if (length > MAX_RESPONSE_BYTES) {
            await reader.cancel();
            throw new Error(`${label}: response exceeds the size limit.`);
          }
          chunks.push(value);
        }
      } finally { reader.releaseLock(); }
    }
    return { response, text: Buffer.concat(chunks, length).toString('utf8') };
  } catch (error) {
    if (signal?.aborted) throw new Error(`${label}: request cancelled.`);
    if (deadline.aborted) throw new Error(`${label}: request timed out.`);
    if (error.message === `${label}: response exceeds the size limit.`) throw error;
    throw new Error(`${label}: could not contact the local service.`);
  }
}

function acceptedRedirect(response, base, label) {
  let location;
  try { location = new URL(response.headers.get('location'), base); } catch { /* Reject below. */ }
  if (response.status !== 303 || !response.headers.has('location') || !location
    || location.origin !== base.origin || location.username || location.password
    || location.pathname !== '/' || location.search || location.hash) {
    throw new Error(`${label}: expected a successful redirect to the local home page (HTTP ${response.status}).`);
  }
}

/** Seed ordinary private records through the authenticated Personal workflow API. */
export async function seedPersonal({ baseUrl, token, signal }) {
  const base = localBase(baseUrl);
  if (typeof token !== 'string' || !token.trim() || /[\r\n]/.test(token)) {
    throw new Error('Personal demo requires its owner session token.');
  }
  async function workflow(feature, input) {
    const label = `Personal demo ${feature}`;
    const { response, text } = await request(base, `/api/v1/personal/${feature}`, {
      method: 'POST', headers: { 'content-type': 'application/json', 'x-tangled-session': token },
      body: JSON.stringify(input),
    }, signal, label);
    if (response.status >= 300 && response.status < 400) throw new Error(`${label}: unexpected redirect.`);
    if (!response.ok) throw new Error(`${label}: local API returned HTTP ${response.status}.`);
    try {
      const value = JSON.parse(text);
      if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error();
      return value;
    } catch { throw new Error(`${label}: local API returned invalid JSON.`); }
  }

  await workflow('notes', {
    action: 'create', rkey: 'demo-welcome', body: {
      title: 'Welcome to your local Tangled demo',
      text: 'Explore Today, save a thought, and try clearing this reminder. These sample records belong to this fresh local demo profile.',
      reminderAt: Math.max(0, Math.floor(Date.now() / 1000) - 60),
    },
  });
  await workflow('notes', {
    action: 'capture', rkey: 'demo-inbox', body: {
      title: 'A thought for your inbox',
      text: 'Try filing this idea as a note: keep your reading, messages and drafts together while choosing what to share.',
    },
  });
  await workflow('capture', {
    action: 'save', kind: 'article', title: 'A library you can carry',
    url: 'https://example.invalid/demo/portable-library',
    text: 'A useful personal library keeps more than a link. It keeps the words you chose to save, the context they came from, and your own notes. This supplied sample article is available offline. Try reading it, adding a tag, or marking it read.',
    tags: ['demo', 'read-later'], source: { type: 'manual', label: 'Supplied local demo content' },
  });
  await workflow('capture', {
    action: 'save', kind: 'text', title: 'A passage worth keeping',
    text: 'Small tools become useful when their records can travel with you. This is a supplied fictional passage for trying the capture reader.',
    tags: ['demo', 'ideas'], source: { type: 'manual', label: 'Supplied local demo content' },
  });
  const feedUrl = 'https://example.invalid/demo/feed.xml';
  await workflow('feeds', { action: 'subscribe', url: feedUrl, title: 'Local demo reading', tags: ['demo'] });
  const feed = await workflow('feeds', {
    action: 'ingest', url: feedUrl,
    xml: '<?xml version="1.0"?><rss version="2.0"><channel><title>Local demo reading</title><link>https://example.invalid/demo</link><description>Supplied fictional articles; no network fetch.</description><item><guid>demo-reading-1</guid><title>Make room for a good idea</title><link>https://example.invalid/demo/ideas</link><description>Keep an inbox for unfinished thoughts. This supplied sample article can be starred, tagged or marked read.</description></item><item><guid>demo-reading-2</guid><title>Your reading, at your pace</title><link>https://example.invalid/demo/reading</link><description>A saved reading queue waits for you. This fictional article was imported from the XML bundled with the local demo.</description></item></channel></rss>',
  });
  const sources = await workflow('connections', {
    action: 'ingest', activity: [{
      key: 'local-demo-message', source: 'demo', account: 'fictional-demo-owner',
      object: {
        id: 'demo-message-1', kind: 'message', conversation: 'demo-planning',
        sender: 'fictional-alex', timestamp: '2026-01-01T10:00:00Z',
        text: 'Fictional demo message: shall we collect our reading ideas here? Try organizing this supplied sample in Connections.',
        attachments: [], data: { fixture: 'local-demo' },
      },
    }],
  });
  if (!Number.isSafeInteger(feed.imported) || feed.imported < 0 || !Array.isArray(sources.items)) {
    throw new Error('Personal demo: local API returned an invalid import summary.');
  }
  return { notes: 2, captures: 2, subscriptions: 1, feedItems: feed.imported, sourceItems: sources.items.length };
}

/** Create a local custodial demo account and publish one sample post. */
export async function seedProvider({ baseUrl, signal }) {
  const base = localBase(baseUrl);
  const email = 'demo@tangled.invalid';
  const password = randomBytes(24).toString('base64url');
  const formHeaders = { 'content-type': 'application/x-www-form-urlencoded' };
  const signup = await request(base, '/signup', {
    method: 'POST', headers: formHeaders,
    body: new URLSearchParams({ email, password, name: 'Demo Explorer' }).toString(),
  }, signal, 'Provider demo signup');
  acceptedRedirect(signup.response, base, 'Provider demo signup');
  const cookies = signup.response.headers.getSetCookie();
  const cookie = cookies.map((value) => value.split(';', 1)[0])
    .find((value) => /^tangled_account=[A-Za-z0-9_-]+$/.test(value));
  if (!cookie) throw new Error('Provider demo signup: local service did not create a sign-in session.');
  const post = await request(base, '/posts', {
    method: 'POST', headers: { ...formHeaders, cookie },
    body: new URLSearchParams({ text: 'Hello from the local Tangled demo! This sample post lives in a fresh provider account. Explore the feed, write a post, and try the account portability tools.' }).toString(),
  }, signal, 'Provider demo post');
  acceptedRedirect(post.response, base, 'Provider demo post');
  return { email, password, posts: 1 };
}
