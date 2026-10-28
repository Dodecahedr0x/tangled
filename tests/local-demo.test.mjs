import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer, connect } from 'node:net';
import { mkdtemp, mkdir, writeFile, readFile, chmod, rm, readdir, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const script = resolve(process.env.TANGLED_DEMO_SCRIPT || fileURLToPath(new URL('../scripts/local-demo.mjs', import.meta.url)));
const fixture = fileURLToPath(new URL('./fixtures/local-demo-tool.mjs', import.meta.url));
const runtime = ['tangled-knot', 'tangled-personal', 'tangled-provider', 'npm', 'reed', 'loomed', 'weft'];
const seeds = ['reed-seed', 'loomed-seed', 'weft-seed'];
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const alive = pid => { try { process.kill(pid, 0); return true; } catch { return false; } };
const close = server => new Promise(resolve => server.close(resolve));

async function availableOffset() {
  for (let attempt = 0; attempt < 100; attempt++) {
    const offset = 10000 + Math.floor(Math.random() * 35000);
    const servers = [];
    try {
      for (const port of [3000, 8788, 8787, 4321]) {
        const server = createServer();
        servers.push(server);
        await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port + offset, port === 8787 ? '0.0.0.0' : '127.0.0.1', resolve); });
      }
      await Promise.all(servers.map(close));
      return offset;
    } catch {
      await Promise.all(servers.filter(s => s.listening).map(close));
    }
  }
  throw new Error('Could not find four unused local fixture ports');
}

async function setup(t, { settings = {}, flags = [], omit = [], noOpen = true, noDesktop = false } = {}) {
  const dir = await mkdtemp(join(tmpdir(), 'tangled-demo-test-'));
  const bin = join(dir, 'bin');
  const target = join(dir, 'target');
  const eventsPath = join(dir, 'events.jsonl');
  const controlPath = join(dir, 'control.json');
  await Promise.all([mkdir(bin), mkdir(join(target, 'debug'), { recursive: true }), writeFile(eventsPath, ''), writeFile(controlPath, JSON.stringify(settings))]);
  for (const role of ['cargo', 'npm', 'open', 'xdg-open', ...runtime.filter(x => x !== 'npm'), ...seeds]) {
    if (omit.includes(role)) continue;
    const path = join(['cargo', 'npm', 'open', 'xdg-open'].includes(role) ? bin : join(target, 'debug'), role);
    await writeFile(path, `#!${process.execPath}\nprocess.argv.splice(2, 0, ${JSON.stringify(role)});\nawait import(${JSON.stringify(pathToFileURL(fixture).href)});\n`);
    await chmod(path, 0o755);
  }
  const offset = await availableOffset();
  let child;
  let output = '';
  let result;
  let exitPromise;
  const events = async () => (await readFile(eventsPath, 'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse);
  const ownedPids = async () => [...new Set((await events()).flatMap(e => e.event === 'start' ? [e.pid] : e.event === 'grandchild' ? [e.child] : []))];
  t.after(async () => {
    if (child && !result) child.kill('SIGKILL');
    // Only PIDs recorded by this fixture, never system-wide name/port matching.
    for (const pid of await ownedPids()) { if (alive(pid)) { try { process.kill(pid, 'SIGKILL'); } catch {} } }
    if (exitPromise) await Promise.race([exitPromise, delay(1000)]);
    await rm(dir, { recursive: true, force: true });
  });
  const run = (extra = []) => {
    assert.equal(child, undefined, 'one supervisor per test fixture');
    child = spawn(process.execPath, [script, '--skip-build', '--port-offset', String(offset), '--data-root', join(dir, 'data'), ...(noOpen ? ['--no-open'] : []), ...(noDesktop ? ['--no-desktop'] : []), ...flags, ...extra], {
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, CARGO_TARGET_DIR: target, DEMO_FIXTURE_EVENTS: eventsPath, DEMO_FIXTURE_CONTROL: controlPath },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    child.stdout.on('data', bytes => { output += bytes; });
    child.stderr.on('data', bytes => { output += bytes; });
    exitPromise = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => { result = { code, signal }; resolve(result); }); });
    return child;
  };
  const until = async (predicate, description, timeout = 15000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await predicate()) return;
      if (result) throw new Error(`Supervisor exited before ${description}: ${JSON.stringify(result)}\n${output}`);
      await delay(30);
    }
    throw new Error(`Timed out waiting for ${description}\n${output}`);
  };
  const exited = async () => {
    let timer;
    try {
      return await Promise.race([exitPromise, new Promise((resolve, reject) => {
        timer = setTimeout(() => reject(new Error(`Supervisor did not exit\n${output}`)), 15000);
      })]);
    } finally { clearTimeout(timer); }
  };
  const cleaned = async ({ excludeOpeners = false } = {}) => {
    const openerPids = excludeOpeners
      ? (await events()).filter(e => e.event === 'start' && ['open', 'xdg-open'].includes(e.role)).map(e => e.pid)
      : [];
    const pids = (await ownedPids()).filter(pid => !openerPids.includes(pid));
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline && pids.some(alive)) await delay(30);
    assert.deepEqual(pids.filter(alive), [], `owned processes leaked\n${output}`);
  };
  return { dir, offset, events, run, until, exited, cleaned, control: settings => writeFile(controlPath, JSON.stringify(settings)), output: () => output, ready: () => until(() => /Demo ready\./i.test(output), 'demo readiness') };
}

for (const stop of ['Enter', 'EOF', 'SIGINT', 'SIGTERM', 'SIGHUP']) {
  test(`${stop} shuts down every owned process`, { timeout: 30000 }, async t => {
    const demo = await setup(t);
    const child = demo.run();
    await demo.ready();
    await demo.until(async () => {
      const started = (await demo.events()).filter(e => e.event === 'start').map(e => e.role);
      return [...runtime, ...seeds].every(role => started.includes(role));
    }, 'all runtime processes to start');
    const roles = (await demo.events()).filter(e => e.event === 'start').map(e => e.role);
    for (const role of [...runtime, ...seeds]) assert.ok(roles.includes(role), `${role} was not started`);
    assert.ok(!roles.includes('open') && !roles.includes('xdg-open'), '--no-open launched a browser opener');
    assert.ok(!(await demo.events()).some(e => e.event === 'unexpected-build'), '--skip-build ran a build');
    if (stop === 'Enter') child.stdin.write('\n');
    else if (stop === 'EOF') child.stdin.end();
    else child.kill(stop);
    assert.equal((await demo.exited()).code, 0, demo.output());
    await demo.cleaned();
  });
}

test('default launch opens interfaces and creates private isolated data', { timeout: 30000 }, async t => {
  const demo = await setup(t, { noOpen: false });
  const child = demo.run();
  await demo.ready();
  await demo.until(async () => (await demo.events()).some(e => ['open', 'xdg-open'].includes(e.role)), 'browser opener');
  const runs = await readdir(join(demo.dir, 'data'));
  assert.equal(runs.length, 1);
  const runDir = join(demo.dir, 'data', runs[0]);
  assert.match(runs[0], /^run-/);
  assert.equal((await stat(runDir)).mode & 0o777, 0o700);
  assert.match(demo.output(), new RegExp(`127\\.0\\.0\\.1:${3000 + demo.offset}`));
  assert.doesNotMatch(demo.output(), /a{64}/, 'private Personal session token escaped into terminal output');
  child.stdin.end();
  assert.equal((await demo.exited()).code, 0, demo.output());
  await demo.cleaned();
});

test('a persistent browser opener survives shutdown without blocking readiness', { timeout: 30000 }, async t => {
  const demo = await setup(t, { noOpen: false, settings: { openerStaysAlive: true } });
  const child = demo.run();
  await demo.ready();
  await demo.until(async () => (await demo.events()).filter(e =>
    e.event === 'start' && ['open', 'xdg-open'].includes(e.role)).length === 3, 'three browser openers');
  const openerPids = (await demo.events()).filter(e =>
    e.event === 'start' && ['open', 'xdg-open'].includes(e.role)).map(e => e.pid);
  assert.ok(openerPids.every(alive), 'persistent browser opener exited before supervisor shutdown');
  child.stdin.end();
  assert.equal((await demo.exited()).code, 0, demo.output());
  await demo.cleaned({ excludeOpeners: true });
  assert.ok(openerPids.every(alive), 'supervisor killed a browser opener during cleanup');
  // setup teardown kills these test-owned fake browsers after this assertion.
});

test('--no-desktop starts only HTTP services and seeds', { timeout: 30000 }, async t => {
  const demo = await setup(t, { noDesktop: true });
  const child = demo.run();
  await demo.ready();
  const roles = (await demo.events()).filter(e => e.event === 'start').map(e => e.role);
  for (const role of ['reed', 'loomed', 'weft']) assert.ok(!roles.includes(role));
  for (const role of runtime.slice(0, 4)) assert.ok(roles.includes(role));
  child.stdin.end();
  assert.equal((await demo.exited()).code, 0);
  await demo.cleaned();
});

test('seed failure aborts startup and cleans up services', { timeout: 30000 }, async t => {
  const demo = await setup(t, { settings: { seedFailure: 'reed-seed' } });
  demo.run();
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  assert.doesNotMatch(demo.output(), /Demo ready\./i);
  await demo.cleaned();
});

test('service failure during startup aborts and cleans up', { timeout: 30000 }, async t => {
  const demo = await setup(t, { settings: { startupFailure: 'tangled-personal' } });
  demo.run();
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  assert.doesNotMatch(demo.output(), /Demo ready\./i);
  await demo.cleaned();
});

test('service failure after readiness exits nonzero and stops siblings', { timeout: 30000 }, async t => {
  const demo = await setup(t);
  demo.run();
  await demo.ready();
  await demo.control({ exit: 'tangled-provider', exitCode: 17 });
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  await demo.cleaned();
});

test('closing a desktop app cleanly stops the entire demo', { timeout: 30000 }, async t => {
  const demo = await setup(t);
  demo.run();
  await demo.ready();
  await demo.control({ exit: 'loomed', exitCode: 0 });
  assert.equal((await demo.exited()).code, 0, demo.output());
  await demo.cleaned();
});

test('cleanup kills a stubborn grandchild after its service leader exits', { timeout: 30000 }, async t => {
  const demo = await setup(t, { settings: { grandchild: 'tangled-knot' } });
  demo.run();
  await demo.ready();
  await demo.until(async () => (await demo.events()).some(e => e.role === 'grandchild' && e.event === 'start'), 'grandchild');
  await demo.control({ leaderExit: 'tangled-knot' });
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  await demo.cleaned();
});

test('occupied ports fail without killing an unrelated listener', { timeout: 30000 }, async t => {
  const demo = await setup(t);
  const occupant = createServer(socket => socket.end('still alive'));
  await new Promise((resolve, reject) => { occupant.once('error', reject); occupant.listen(3000 + demo.offset, '127.0.0.1', resolve); });
  t.after(() => close(occupant));
  demo.run();
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  assert.ok(occupant.listening);
  const received = await new Promise((resolve, reject) => { const socket = connect(3000 + demo.offset, '127.0.0.1'); let text = ''; socket.on('data', value => { text += value; }); socket.on('end', () => resolve(text)); socket.on('error', reject); });
  assert.equal(received, 'still alive');
  assert.ok(!(await demo.events()).some(e => runtime.includes(e.role)), 'started services despite occupied port');
  await demo.cleaned();
});

test('a wildcard provider listener is rejected before launching tools', { timeout: 30000 }, async t => {
  const demo = await setup(t);
  const occupant = createServer(socket => socket.destroy());
  await new Promise((resolve, reject) => { occupant.once('error', reject); occupant.listen(8787 + demo.offset, '0.0.0.0', resolve); });
  t.after(() => close(occupant));
  demo.run();
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  assert.match(demo.output(), /provider.*occupied|occupied.*provider/i);
  assert.deepEqual(await demo.events(), [], 'started tools despite an occupied provider port');
  assert.ok(occupant.listening, 'existing provider was stopped');
  await demo.cleaned();
});

test('a port claimed during preparation is rejected before starting services', { timeout: 30000 }, async t => {
  const demo = await setup(t, { settings: { holdMetadata: true } });
  demo.run();
  await demo.until(async () => (await demo.events()).some(e => e.role === 'cargo' && e.event === 'start'), 'metadata preparation');
  const occupant = createServer(socket => socket.destroy());
  await new Promise((resolve, reject) => { occupant.once('error', reject); occupant.listen(8787 + demo.offset, '0.0.0.0', resolve); });
  t.after(() => close(occupant));
  await demo.control({});
  assert.notEqual((await demo.exited()).code, 0, demo.output());
  assert.ok(!(await demo.events()).some(e => e.event === 'start' && runtime.includes(e.role)), 'started services after their port was claimed');
  assert.ok(occupant.listening, 'existing provider was stopped');
  await demo.cleaned();
});

test('provider readiness requires its own successful listener acknowledgment', { timeout: 30000 }, async t => {
  const demo = await setup(t, { settings: { providerReadyMarker: false } });
  const child = demo.run();
  await demo.until(async () => (await demo.events()).some(e => e.role === 'tangled-provider' && e.event === 'listening'), 'provider listener');
  await delay(700);
  const events = await demo.events();
  assert.ok(!events.some(e => e.role === 'tangled-provider' && e.event === 'request'), 'contacted a provider before its listener acknowledgment');
  assert.ok(!events.some(e => seeds.includes(e.role) && e.event === 'start'), 'seeded before owned provider readiness');
  assert.doesNotMatch(demo.output(), /Demo ready\./i);
  child.kill('SIGINT');
  assert.equal((await demo.exited()).code, 0, demo.output());
  await demo.cleaned();
});

test('missing binaries fail with a useful diagnostic and no leaked processes', { timeout: 30000 }, async t => {
  const demo = await setup(t, { omit: ['tangled-knot'] });
  demo.run();
  assert.notEqual((await demo.exited()).code, 0);
  assert.match(demo.output(), /tangled-knot/);
  await demo.cleaned();
});

for (const flags of [['--unknown'], ['--port-offset', '-1'], ['--port-offset', 'abc'], ['--port-offset', '60000'], ['--data-root'], ['--port-offset']]) {
  test(`invalid flags ${flags.join(' ')} fail before launching tools`, { timeout: 10000 }, async t => {
    const demo = await setup(t, { flags });
    demo.run();
    assert.notEqual((await demo.exited()).code, 0);
    assert.equal((await demo.events()).length, 0, demo.output());
  });
}

test('--help documents supported flags without launching tools', { timeout: 10000 }, async t => {
  const demo = await setup(t);
  demo.run(['--help']);
  assert.equal((await demo.exited()).code, 0, demo.output());
  for (const flag of ['--no-open', '--no-desktop', '--skip-build', '--port-offset', '--data-root']) assert.ok(demo.output().includes(flag));
  assert.equal((await demo.events()).length, 0);
});

test('a browser opener failure reports manual links without stopping services', { timeout: 15000 }, async t => {
  const demo = await setup(t, { noOpen: false, settings: { openerExitCode: 7 } });
  const child = demo.run();
  await demo.ready();
  await demo.until(() => /Could not open/.test(demo.output()) && /launch-links\.json/.test(demo.output()), 'browser failure warning', 3000);
  child.stdin.end();
  assert.equal((await demo.exited()).code, 0, demo.output());
  await demo.cleaned();
});
