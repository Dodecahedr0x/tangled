import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import test from 'node:test';
import { seedPersonal, seedProvider } from '../scripts/local-demo-seed.mjs';

async function server(t, handler) {
  const service = createServer((req, res) => {
    Promise.resolve(handler(req, res)).catch((error) => {
      res.writeHead(500).end(String(error));
    });
  });
  service.listen(0, '127.0.0.1');
  await once(service, 'listening');
  t.after(() => { service.closeAllConnections(); service.close(); });
  return `http://127.0.0.1:${service.address().port}`;
}

async function body(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  return Buffer.concat(chunks).toString();
}

function json(res, value) {
  res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(value));
}

test('Personal seeds due/inbox, offline captures, supplied RSS and synthetic activity through JSON workflows', async (t) => {
  const requests = [];
  const baseUrl = await server(t, async (req, res) => {
    assert.equal(req.method, 'POST');
    assert.equal(req.headers['x-tangled-session'], 'private-owner-token');
    assert.equal(req.headers['content-type'], 'application/json');
    const input = JSON.parse(await body(req));
    requests.push({ path: req.url, input });
    if (req.url.endsWith('/feeds') && input.action === 'ingest') {
      json(res, { imported: 2, skipped: 0, items: [{}, {}] });
    } else if (req.url.endsWith('/connections')) {
      json(res, { items: input.activity.map(() => ({})) });
    } else {
      json(res, { uri: 'tangled://demo/collection/key', cid: 'demo-cid', body: input.body ?? {} });
    }
  });
  const summary = await seedPersonal({ baseUrl, token: 'private-owner-token' });
  assert.deepEqual(summary, { notes: 2, captures: 2, subscriptions: 1, feedItems: 2, sourceItems: 1 });
  const notes = requests.filter(({ path }) => path.endsWith('/notes')).map(({ input }) => input);
  assert.equal(notes.length, 2);
  assert.ok(notes.some((note) => note.action === 'capture' && note.body.text));
  assert.ok(notes.some((note) => note.body.reminderAt <= Math.floor(Date.now() / 1000)));
  const captures = requests.filter(({ path }) => path.endsWith('/capture')).map(({ input }) => input);
  assert.deepEqual(captures.map((item) => item.kind).sort(), ['article', 'text']);
  assert.ok(captures.every((item) => item.action === 'save' && item.text.length > 20));
  const feeds = requests.filter(({ path }) => path.endsWith('/feeds')).map(({ input }) => input);
  assert.equal(feeds[0].action, 'subscribe');
  assert.equal(feeds[1].action, 'ingest');
  assert.equal(feeds[0].url, feeds[1].url);
  assert.match(feeds[1].xml, /<rss[\s>]/);
  assert.equal((feeds[1].xml.match(/<item>/g) ?? []).length, 2);
  const activity = requests.find(({ path }) => path.endsWith('/connections')).input.activity;
  assert.equal(activity.length, 1);
  assert.match(activity[0].object.text, /demo|sample|fictional/i);
  assert.equal(activity[0].object.kind, 'message');
  assert.ok(activity[0].source && activity[0].account && activity[0].object.id);
  assert.equal(requests.length, 7);
});

test('provider signup and post use its HTML form contract and session cookie', async (t) => {
  let credentials;
  const paths = [];
  const baseUrl = await server(t, async (req, res) => {
    paths.push(req.url);
    assert.equal(req.method, 'POST');
    assert.match(req.headers['content-type'], /^application\/x-www-form-urlencoded/);
    const form = new URLSearchParams(await body(req));
    if (req.url === '/signup') {
      assert.equal(req.headers.cookie, undefined);
      assert.equal(form.get('email'), 'demo@tangled.invalid');
      assert.ok(form.get('name'));
      assert.match(form.get('password'), /^[A-Za-z0-9_-]{32,}$/);
      credentials = Object.fromEntries(form);
      res.writeHead(303, { location: '/', 'set-cookie': 'tangled_account=session-secret; HttpOnly; Path=/; SameSite=Lax' }).end();
    } else if (req.url === '/posts') {
      assert.equal(req.headers.cookie, 'tangled_account=session-secret');
      assert.match(form.get('text'), /demo|sample/i);
      res.writeHead(303, { location: '/' }).end();
    } else {
      assert.fail('unexpected request');
    }
  });
  const result = await seedProvider({ baseUrl });
  assert.equal(result.email, credentials.email);
  assert.equal(result.password, credentials.password);
  assert.equal(result.posts, 1);
  assert.deepEqual(paths, ['/signup', '/posts']);
});

test('provider generates distinct random passwords for fresh runs', async (t) => {
  const passwords = [];
  const baseUrl = await server(t, async (req, res) => {
    if (req.url === '/signup') {
      passwords.push(new URLSearchParams(await body(req)).get('password'));
      res.writeHead(303, { location: '/', 'set-cookie': 'tangled_account=local-session; Path=/' }).end();
    } else res.writeHead(303, { location: '/' }).end();
  });
  await seedProvider({ baseUrl });
  await seedProvider({ baseUrl });
  assert.notEqual(passwords[0], passwords[1]);
});

test('Personal rejects redirects without forwarding its owner credential', async (t) => {
  let leaked = false;
  const destination = await server(t, (req, res) => { leaked = true; res.end('{}'); });
  const baseUrl = await server(t, (req, res) => res.writeHead(307, { location: destination }).end());
  await assert.rejects(seedPersonal({ baseUrl, token: 'do-not-leak' }), /Personal.*redirect/i);
  assert.equal(leaked, false);
});

test('provider rejects foreign signup and post redirects without forwarding secrets', async (t) => {
  let leaked = false;
  const destination = await server(t, (req, res) => { leaked = true; res.end(); });
  for (const redirectAt of ['/signup', '/posts']) {
    const baseUrl = await server(t, (req, res) => {
      const headers = { location: req.url === redirectAt ? destination : '/' };
      if (req.url === '/signup') headers['set-cookie'] = 'tangled_account=secret; Path=/';
      res.writeHead(303, headers).end();
    });
    await assert.rejects(seedProvider({ baseUrl }), /provider.*redirect/i);
  }
  assert.equal(leaked, false);
});

test('provider requires a successful signup session and successful post redirect', async (t) => {
  for (const failure of ['signup-status', 'cookie', 'post-login', 'post-status', 'post-html']) {
    const baseUrl = await server(t, (req, res) => {
      if (req.url === '/signup') {
        if (failure === 'signup-status') return res.writeHead(400).end('password=secret');
        const headers = { location: '/' };
        if (failure !== 'cookie') headers['set-cookie'] = 'tangled_account=secret; Path=/';
        return res.writeHead(303, headers).end();
      }
      if (failure === 'post-status') return res.writeHead(502).end('session=secret');
      if (failure === 'post-html') return res.end('<p>Write a post</p>');
      res.writeHead(303, { location: '/login' }).end();
    });
    await assert.rejects(seedProvider({ baseUrl }), (error) => {
      assert.match(error.message, /provider/i);
      assert.doesNotMatch(error.message, /secret|password=/);
      return true;
    });
  }
});

test('Personal HTTP/JSON failures have useful errors without echoing response secrets', async (t) => {
  for (const failure of ['status', 'json', 'oversized']) {
    const baseUrl = await server(t, (req, res) => {
      if (failure === 'status') res.writeHead(401).end('{"error":"private-owner-token"}');
      if (failure === 'json') res.end('private-owner-token');
      if (failure === 'oversized') res.end('x'.repeat(2 * 1024 * 1024));
    });
    await assert.rejects(seedPersonal({ baseUrl, token: 'private-owner-token' }), (error) => {
      assert.match(error.message, /Personal/);
      assert.doesNotMatch(error.message, /private-owner-token/);
      if (failure === 'status') assert.match(error.message, /401/);
      if (failure === 'json') assert.match(error.message, /JSON/i);
      if (failure === 'oversized') assert.match(error.message, /large|limit/i);
      return true;
    });
  }
});

test('both seeders honor aborts during requests and do not leak abort reasons', async (t) => {
  for (const seed of [seedPersonal, seedProvider]) {
    const started = Promise.withResolvers();
    const baseUrl = await server(t, (req) => { req.resume(); started.resolve(); });
    const controller = new AbortController();
    const work = seed({ baseUrl, token: 'private-owner-token', signal: controller.signal });
    await started.promise;
    controller.abort(new Error('private-owner-token'));
    await assert.rejects(work, (error) => {
      assert.match(error.message, /abort|cancel/i);
      assert.doesNotMatch(error.message, /private-owner-token/);
      return true;
    });
  }
});

test('both seeders abort while reading a stalled response body', async (t) => {
  for (const seed of [seedPersonal, seedProvider]) {
    const started = Promise.withResolvers();
    const baseUrl = await server(t, (req, res) => {
      req.resume();
      res.writeHead(200, { 'content-type': 'application/json' });
      res.write('{');
      started.resolve();
    });
    const controller = new AbortController();
    const work = seed({ baseUrl, token: 'private-owner-token', signal: controller.signal });
    await started.promise;
    controller.abort();
    await assert.rejects(work, /abort|cancel/i);
  }
});

test('seeding accepts only credential-free numeric loopback HTTP base URLs', async () => {
  for (const baseUrl of ['https://example.com', 'http://localhost:8788', 'http://user:secret@127.0.0.1:1', 'http://127.0.0.1:1/path', 'http://127.0.0.1:1/?secret=1']) {
    await assert.rejects(seedPersonal({ baseUrl, token: 'token' }), /loopback|base URL/i);
    await assert.rejects(seedProvider({ baseUrl }), /loopback|base URL/i);
  }
  await assert.rejects(seedPersonal({ baseUrl: 'http://127.0.0.1:1', token: '' }), /session token/i);
});

test('both seeders bound stalled requests without a caller deadline', { timeout: 15_000 }, async (t) => {
  const baseUrl = await server(t, (req) => req.resume());
  const started = Date.now();
  await Promise.all([seedPersonal({ baseUrl, token: 'token' }), seedProvider({ baseUrl })].map(async (work) => {
    await assert.rejects(work, /timed out/i);
  }));
  assert.ok(Date.now() - started < 14_000);
});

test('an already aborted seed sends no requests or secret abort reason', async (t) => {
  let requests = 0;
  const baseUrl = await server(t, (req, res) => { requests++; res.end('{}'); });
  const controller = new AbortController();
  controller.abort(new Error('private-secret'));
  for (const seed of [seedPersonal, seedProvider]) {
    await assert.rejects(seed({ baseUrl, token: 'token', signal: controller.signal }), (error) => {
      assert.match(error.message, /cancel/i);
      assert.doesNotMatch(error.message, /private-secret/);
      return true;
    });
  }
  assert.equal(requests, 0);
});

test('Personal rejects incomplete import responses instead of reporting false success', async (t) => {
  const baseUrl = await server(t, (req, res) => { req.resume(); json(res, {}); });
  await assert.rejects(seedPersonal({ baseUrl, token: 'token' }), /invalid import summary/i);
});

test('provider ignores unrelated cookies and rejects malformed session cookies', async (t) => {
  for (const cookie of ['other=session; Path=/', 'tangled_account=; Path=/', 'tangled_account=bad,value; Path=/']) {
    let requests = 0;
    const baseUrl = await server(t, (req, res) => {
      requests++;
      req.resume();
      res.writeHead(303, { location: '/', 'set-cookie': cookie }).end();
    });
    await assert.rejects(seedProvider({ baseUrl }), /sign-in session/i);
    assert.equal(requests, 1);
  }
});
