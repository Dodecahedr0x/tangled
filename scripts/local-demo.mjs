#!/usr/bin/env node
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import path from 'node:path';
import net from 'node:net';
import readline from 'node:readline';
import { spawn } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const HELP = `Tangled local demo (Node 22+, macOS/Linux)\n\nUsage: just demo [flags]\n   or: node scripts/local-demo.mjs [flags]\n\nStarts and seeds Knot, Personal, provider, docs, Reed, Loomed and Weft.\nPress Enter or Ctrl-C to stop all owned processes. Closing a desktop app also stops the demo.\nData and private logs are retained in a fresh .data/local-demo/run-* directory.\n\n  --no-open         Print browser links instead of opening browser tabs\n  --no-desktop      Skip building/opening the three native desktop apps\n  --skip-build      Reuse existing binaries and documentation dependencies\n  --port-offset N   Add N to ports 3000, 8788, 8787 and 4321\n  --data-root DIR   Parent directory for a fresh private demo run\n  --help            Show this help\n`;

export function parseOptions(args, root = ROOT) {
  const options = { noOpen: false, noDesktop: false, skipBuild: false, portOffset: 0, dataRoot: path.join(root, '.data/local-demo'), help: false };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--help' || arg === '-h') options.help = true;
    else if (arg === '--no-open') options.noOpen = true;
    else if (arg === '--no-desktop') options.noDesktop = true;
    else if (arg === '--skip-build') options.skipBuild = true;
    else if (arg === '--port-offset' || arg === '--data-root') {
      const value = args[++i];
      if (!value || value.startsWith('--')) throw new Error(`${arg} needs a value.`);
      if (arg === '--data-root') options.dataRoot = path.resolve(value);
      else {
        if (!/^\d+$/.test(value) || Number(value) > 56747) throw new Error('Port offset must be an integer from 0 to 56747.');
        options.portOffset = Number(value);
      }
    } else throw new Error(`Unknown option: ${arg}. Use --help.`);
  }
  return options;
}
export function demoPorts(offset = 0) {
  return { knot: 3000 + offset, personal: 8788 + offset, provider: 8787 + offset, docs: 4321 + offset };
}

function groupExists(record) {
  if (record.retired || !record.child.pid) return false;
  try { process.kill(-record.child.pid, 0); return true; }
  catch (error) {
    if (error.code === 'ESRCH') { record.retired = true; return false; }
    // EPERM on a zero-signal probe means the group may still exist. Keep
    // waiting conservatively instead of declaring cleanup failed mid-exit.
    if (error.code === 'EPERM') return true;
    throw new Error(`${record.label} process group ${record.child.pid}: ${error.code}`);
  }
}
function signalGroup(record, signal) {
  if (!groupExists(record)) return;
  try { process.kill(-record.child.pid, signal); }
  catch (error) { if (error.code !== 'ESRCH') throw new Error(`${record.label} process group ${record.child.pid}: ${signal} failed (${error.code})`); }
}
async function cleanup(records) {
  for (const record of records) signalGroup(record, 'SIGTERM');
  const grace = Date.now() + 2000;
  while (Date.now() < grace && records.some(groupExists)) await delay(50);
  for (const record of records) signalGroup(record, 'SIGKILL');
  let reapTimer;
  try {
    await Promise.race([
      Promise.all(records.map(record => record.done)),
      new Promise((_, reject) => { reapTimer = setTimeout(() => reject(new Error('A demo process did not exit after forced shutdown.')), 2000); }),
    ]);
  } finally { clearTimeout(reapTimer); }
  const deadline = Date.now() + 2000;
  while (Date.now() < deadline && records.some(groupExists)) await delay(50);
  if (records.some(groupExists)) throw new Error('A demo process group did not stop; inspect the private logs.');
}
async function freePorts(ports) {
  const probes = [];
  try {
    for (const [name, port] of Object.entries(ports)) {
      const server = net.createServer(); probes.push(server);
      try { await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, name === 'provider' ? '0.0.0.0' : '127.0.0.1', resolve); }); }
      catch { throw new Error(`Port ${port} for ${name} is occupied or unavailable. Stop that service or use --port-offset N. Existing processes were left running.`); }
    }
  } finally { await Promise.all(probes.map(server => new Promise(resolve => server.close(() => resolve())))); }
}

export async function main(args = process.argv.slice(2)) {
  const options = parseOptions(args);
  if (options.help) { console.log(HELP); return 0; }
  if (Number(process.versions.node.split('.')[0]) < 22) throw new Error('Use Node.js 22 or later.');
  if (!['darwin', 'linux'].includes(process.platform)) throw new Error('The managed local demo supports macOS and Linux; use the README manual setup on this platform.');
  const ports = demoPorts(options.portOffset);
  const urls = Object.fromEntries(Object.entries(ports).map(([name, port]) => [name, `http://127.0.0.1:${port}`]));
  const controller = new AbortController(), records = [];
  let stopping = false, exitCode = 0, resolveStop, runRoot;
  const stopped = new Promise(resolve => { resolveStop = resolve; });
  function stop(reason, code = 0) {
    if (stopping) return;
    stopping = true; exitCode = code;
    console.log(`Stopping: ${reason}`);
    controller.abort(new Error('Demo stopped.')); resolveStop();
  }
  const handlers = Object.fromEntries(['SIGINT', 'SIGTERM', 'SIGHUP'].map(signal => [signal, () => stop(signal)]));
  for (const [signal, handler] of Object.entries(handlers)) process.on(signal, handler);
  const input = readline.createInterface({ input: process.stdin, terminal: false });
  input.on('line', () => stop('terminal input'));
  input.on('close', () => stop('terminal closed'));
  function check() { if (stopping) throw new Error('Demo stopped.'); }
  function launch(label, command, argv, { env = {}, runtime = false, desktop = false, capture = false } = {}) {
    check();
    const log = path.join(runRoot, 'logs', label.replace(/[^a-z0-9-]/gi, '-').toLowerCase() + '.log');
    const fd = fs.openSync(log, 'a', 0o600);
    let child;
    try { child = spawn(command, argv, { cwd: ROOT, env: { ...process.env, ...env }, detached: true, stdio: ['ignore', capture ? 'pipe' : fd, fd] }); }
    finally { fs.closeSync(fd); }
    const record = { child, log, label, output: '', result: null, retired: false }; records.push(record);
    if (capture) {
      child.stdout.setEncoding('utf8');
      child.stdout.on('data', chunk => { fs.appendFileSync(log, chunk, { mode: 0o600 }); record.output = (record.output + chunk).slice(-2 * 1024 * 1024); });
    }
    record.done = new Promise(resolve => {
      function finish(result) {
        if (record.result) return;
        record.result = result;
        if (child.pid) groupExists(record);
        resolve(result);
        if (runtime && !stopping) {
          if (desktop && result.code === 0) stop(`${label} closed`);
          else stop(`${label} exited unexpectedly. See ${log}`, 1);
        }
      }
      child.once('error', error => finish({ error, code: 1 }));
      child.once('exit', (code, signal) => finish({ code, signal }));
    });
    record.closed = new Promise(resolve => child.once('close', resolve));
    return record;
  }
  async function run(label, command, argv, settings = {}) {
    console.log(label);
    const record = launch(label, command, argv, { ...settings, capture: !!settings.capture });
    const result = await Promise.race([record.done, stopped.then(() => { throw new Error('Demo stopped.'); })]);
    check();
    if (result.error || result.code !== 0) throw new Error(`${label} failed${result.error?.code === 'ENOENT' ? ': command not found' : ''}. See ${record.log}`);
    // Retire short-lived command groups now, rather than keeping a stale
    // process-group ID for the lifetime of an interactive demo.
    await cleanup([record]);
    if (settings.capture) await Promise.race([record.closed, stopped.then(() => { throw new Error('Demo stopped.'); })]);
    return record.output;
  }
  async function ready(record, url, headers, readyLine) {
    const deadline = Date.now() + 60000;
    while (Date.now() < deadline) {
      check();
      if (record.result) throw new Error(`${record.label} stopped before becoming ready. See ${record.log}`);
      // A reachable URL may belong to an unrelated server. The provider must
      // acknowledge its own successful bind before we send requests or seed it.
      if (readyLine && !record.output.split('\n').includes(readyLine)) { await delay(100); continue; }
      try {
        const response = await fetch(url, { headers, redirect: 'manual', signal: AbortSignal.any([controller.signal, AbortSignal.timeout(1500)]) });
        await response.body?.cancel();
        if (response.ok) { check(); if (!record.result) return; }
      } catch { check(); }
      await delay(100);
    }
    throw new Error(`${record.label} did not become ready within 60 seconds. See ${record.log}`);
  }
  try {
    await freePorts(ports); check();
    await fsp.mkdir(options.dataRoot, { recursive: true, mode: 0o700 });
    runRoot = await fsp.mkdtemp(path.join(options.dataRoot, 'run-'));
    await fsp.mkdir(path.join(runRoot, 'logs'), { mode: 0o700 });
    console.log(`Demo data: ${runRoot}`);
    const appEnv = { TANGLED_KNOT: urls.knot, TANGLED_PERSONAL_URL: urls.personal + '/' };
    for (const app of ['reed', 'loomed', 'weft']) {
      const home = path.join(runRoot, 'apps', app);
      await fsp.mkdir(home, { recursive: true, mode: 0o700 });
      await fsp.writeFile(path.join(home, 'knot.txt'), urls.knot + '\n', { mode: 0o600 });
      appEnv[`${app.toUpperCase()}_HOME`] = home;
    }
    if (!options.skipBuild) {
      await run('Build services and sample tools', 'cargo', ['build', '-p', 'tangled-knot', '-p', 'tangled-personal', '-p', 'tangled-provider', '-p', 'reed-core', '-p', 'loomed-core', '-p', 'weft-core', '--bins']);
      if (!options.noDesktop) await run('Build desktop apps', 'cargo', ['build', '-p', 'reed', '-p', 'loomed', '-p', 'weft', '--features', 'reed/desktop,loomed/desktop,weft/desktop', '--bins'], { env: appEnv });
      if (!fs.existsSync(path.join(ROOT, 'site/node_modules/astro/package.json'))) await run('Install documentation dependencies', 'npm', ['--prefix', path.join(ROOT, 'site'), 'ci', '--no-audit', '--no-fund']);
    }
    const metadata = JSON.parse(await run('Locate build output', 'cargo', ['metadata', '--offline', '--no-deps', '--format-version', '1'], { capture: true }));
    const target = path.join(metadata.target_directory, 'debug');
    const required = ['tangled-knot', 'tangled-personal', 'tangled-provider', 'reed-seed', 'loomed-seed', 'weft-seed', ...(options.noDesktop ? [] : ['reed', 'loomed', 'weft'])];
    for (const name of required) {
      try { await fsp.access(path.join(target, name), fs.constants.X_OK); }
      catch { throw new Error(`Missing executable ${name}. Run again without --skip-build.`); }
    }
    // Builds can take minutes; a port may have been claimed since preflight.
    await freePorts(ports); check();
    const knot = launch('Knot', path.join(target, 'tangled-knot'), ['--listen', `127.0.0.1:${ports.knot}`, '--data', path.join(runRoot, 'knot')], { runtime: true, env: { TANGLED_PEERS: '' } });
    const personal = launch('Personal', path.join(target, 'tangled-personal'), ['serve', '--data', path.join(runRoot, 'personal'), '--listen', `127.0.0.1:${ports.personal}`, '--no-open'], { runtime: true, capture: true });
    const provider = launch('Provider', path.join(target, 'tangled-provider'), [], { runtime: true, capture: true, env: { PORT: String(ports.provider), DATA_DIR: path.join(runRoot, 'provider'), PUBLIC_URL: urls.provider } });
    const docs = launch('Documentation', 'npm', ['--prefix', path.join(ROOT, 'site'), 'run', 'dev', '--', '--host', '127.0.0.1', '--port', String(ports.docs)], { runtime: true, env: { PUBLIC_PROVIDER_URL: urls.provider, ASTRO_BASE: '/', ASTRO_SITE: urls.docs } });
    await Promise.all([ready(knot, urls.knot + '/xrpc/tangled.node.describe'), ready(provider, urls.provider + '/', undefined, `tangled-provider listening 0.0.0.0:${ports.provider}`), ready(docs, urls.docs + '/')]);
    const tokenDeadline = Date.now() + 15000;
    let personalLink;
    while (!personalLink && Date.now() < tokenDeadline) {
      check();
      personalLink = personal.output.split('\n').find(line => new RegExp(`^http://127\\.0\\.0\\.1:${ports.personal}/#session=[a-f0-9]{64}$`).test(line));
      if (!personalLink) await delay(50);
    }
    if (!personalLink) throw new Error('Personal did not print its private launch link.');
    const token = new URLSearchParams(new URL(personalLink).hash.slice(1)).get('session');
    await ready(personal, urls.personal + '/api/v1/session', { 'X-Tangled-Session': token });
    for (const name of ['reed', 'loomed', 'weft']) await run(`Seed ${name}`, path.join(target, `${name}-seed`), ['--knot', urls.knot, ...(name === 'weft' ? [] : ['--create-local'])], { env: appEnv });
    const { seedPersonal, seedProvider } = await import('./local-demo-seed.mjs');
    console.log('Seed Personal and provider');
    const [personalSample, account] = await Promise.all([seedPersonal({ baseUrl: urls.personal, token, signal: controller.signal }), seedProvider({ baseUrl: urls.provider, signal: controller.signal })]);
    check();
    await fsp.writeFile(path.join(runRoot, 'demo-account.json'), JSON.stringify(account, null, 2) + '\n', { mode: 0o600 });
    await fsp.writeFile(path.join(runRoot, 'launch-links.json'), JSON.stringify({ ...urls, personal: personalLink }, null, 2) + '\n', { mode: 0o600 });
    if (!options.noDesktop) {
      for (const name of ['reed', 'loomed', 'weft']) launch(name, path.join(target, name), [], { runtime: true, desktop: true, env: appEnv });
      await delay(500); check();
    }
    if (!options.noOpen) for (const [name, url] of [['Personal', personalLink], ['Provider', urls.provider], ['Documentation', urls.docs]]) {
      check();
      // Browser launchers may become the user's browser. They must never enter
      // the owned process groups or block readiness while a browser stays open.
      try {
        const opener = spawn(process.platform === 'darwin' ? 'open' : 'xdg-open', [url], { detached: true, stdio: 'ignore' });
        opener.unref();
        opener.once('exit', code => {
          if (code !== 0 && !stopping) console.error(`Could not open ${name}. Use launch-links.json in the private run directory.`);
        });
        await new Promise((resolve, reject) => {
          const timer = setTimeout(() => reject(new Error('Browser opener did not start')), 2000);
          opener.once('spawn', () => { clearTimeout(timer); resolve(); });
          opener.once('error', () => { clearTimeout(timer); reject(new Error('Browser opener unavailable')); });
        });
      } catch { check(); console.error(`Could not open ${name}. Use launch-links.json in the private run directory.`); }
    }
    check();
    console.log(`Knot: ${urls.knot}\nPersonal: ${options.noOpen ? personalLink : urls.personal + ' (private launch link saved in launch-links.json)'}\nProvider: ${urls.provider}\nDocumentation: ${urls.docs}`);
    console.log(`Demo account: ${account.email}\nDemo password: ${account.password}`);
    console.log(`Personal sample: ${personalSample.notes} notes, ${personalSample.captures} captures, ${personalSample.feedItems} feed articles.`);
    console.log('Demo ready. Press Enter or Ctrl-C to stop everything.');
    await stopped;
  } catch (error) {
    if (!stopping) { console.error(error.message || String(error)); stop('startup or runtime failure', 1); }
  } finally {
    if (!stopping) stop('launcher exiting');
    input.close();
    try { await cleanup(records); }
    catch (error) { console.error(error.message); exitCode = 1; }
    for (const [signal, handler] of Object.entries(handlers)) process.removeListener(signal, handler);
    if (runRoot) console.log(`Demo stopped. Data and private logs retained at ${runRoot}`);
  }
  return exitCode;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().then(code => { process.exitCode = code; }).catch(error => { console.error(error.message || String(error)); process.exitCode = 1; });
}
