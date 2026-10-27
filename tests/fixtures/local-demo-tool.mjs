#!/usr/bin/env node
// Real child processes and HTTP listeners used only by local-demo.test.mjs.
import { appendFileSync, readFileSync } from 'node:fs';
import { basename } from 'node:path';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';

const role = process.argv[2];
const args = process.argv.slice(3);
const log = (event, fields = {}) => appendFileSync(process.env.DEMO_FIXTURE_EVENTS,
  `${JSON.stringify({ event, role, pid: process.pid, args, ...fields })}\n`);
const control = () => {
  try { return JSON.parse(readFileSync(process.env.DEMO_FIXTURE_CONTROL, 'utf8')); }
  catch { return {}; }
};
log('start', { env: Object.fromEntries(['PORT', 'REED_HOME', 'LOOMED_HOME', 'WEFT_HOME', 'TANGLED_KNOT', 'TANGLED_PERSONAL_URL'].map(k => [k, process.env[k]])) });

if (role === 'cargo') {
  if (args[0] === 'metadata') {
    console.log(JSON.stringify({ target_directory: process.env.CARGO_TARGET_DIR }));
    process.exit(0);
  }
  log('unexpected-build');
  process.exit(91);
}
if (role === 'npm' && args.includes('ci')) { log('unexpected-build'); process.exit(91); }
if (['open', 'xdg-open'].includes(role)) process.exit(0);
if (role.endsWith('-seed')) {
  process.stdin.resume();
  if (control().seedFailure === role) process.exit(23);
  console.log(JSON.stringify({ ok: true }));
  process.exit(0);
}

if (role === 'grandchild') {
  // Ignore graceful shutdown so cleanup must escalate to the process group.
  process.on('SIGTERM', () => log('ignored-term'));
  setInterval(() => {}, 1000);
} else {
  if (control().grandchild === role) {
    const child = spawn(process.execPath, [new URL(import.meta.url).pathname, 'grandchild'], { stdio: 'ignore' });
    log('grandchild', { child: child.pid });
  }
  let server;
  const defaultPorts = { 'tangled-knot': 3000, 'tangled-personal': 8788, 'tangled-provider': 8787, npm: 4321 };
  if (role in defaultPorts) {
    let port = Number(process.env.PORT || defaultPorts[role]);
    for (let i = 0; i < args.length; i++) {
      if (['--port', '--listen', '--bind'].includes(args[i])) port = Number(args[i + 1].split(':').at(-1));
      if (args[i].startsWith('--port=')) port = Number(args[i].slice(7));
    }
    if (control().startupFailure === role) process.exit(27);
    const token = 'a'.repeat(64);
    server = createServer(async (request, response) => {
      let body = '';
      for await (const chunk of request) body += chunk;
      const path = new URL(request.url, 'http://localhost').pathname;
      log('request', { method: request.method, path, body });
      if (role === 'tangled-personal' && path === '/api/session') {
        response.writeHead(200, { 'content-type': 'application/json', 'set-cookie': 'tangled_session=fixture; HttpOnly; SameSite=Strict; Path=/' });
        response.end(JSON.stringify({ ok: true }));
      } else if (role === 'tangled-personal' && path.startsWith('/api/')) {
        response.writeHead(200, { 'content-type': 'application/json' });
        response.end(JSON.stringify({ ok: true, id: 'fixture-id', record: { id: 'fixture-id' }, records: [], items: [], subscriptions: [], sources: [] }));
      } else if (role === 'tangled-provider' && request.method === 'POST' && ['/signup', '/posts'].includes(path)) {
        response.writeHead(303, { location: '/', 'set-cookie': 'provider_session=fixture; HttpOnly; Path=/' });
        response.end();
      } else {
        response.writeHead(200, { 'content-type': 'text/html' });
        response.end('<!doctype html><title>Local demo fixture</title>');
      }
    });
    server.listen(port, '127.0.0.1', () => {
      log('listening', { port });
      if (role === 'tangled-personal') console.log(`http://127.0.0.1:${port}/#session=${token}`);
    });
    server.on('error', error => { log('server-error', { error: error.message }); process.exit(28); });
  } else if (!['reed', 'loomed', 'weft'].includes(role)) {
    throw new Error(`Unexpected fixture role ${basename(role)}`);
  }
  process.on('SIGTERM', () => { log('term'); process.exit(0); });
  process.on('SIGINT', () => process.exit(0));
  setInterval(() => {
    const settings = control();
    if (settings.exit === role) { log('controlled-exit'); process.exit(settings.exitCode ?? 19); }
    if (settings.leaderExit === role) { log('controlled-exit'); process.exit(19); }
  }, 30);
}
