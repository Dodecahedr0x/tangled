import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { parseOptions, demoPorts } from '../scripts/local-demo.mjs';

test('defaults run all seven processes with isolated fresh state parent', () => {
  const options = parseOptions([], '/project');
  assert.equal(options.noOpen, false); assert.equal(options.noDesktop, false); assert.equal(options.skipBuild, false);
  assert.equal(options.dataRoot, '/project/.data/local-demo');
  assert.deepEqual(demoPorts(options.portOffset), { knot:3000, personal:8788, provider:8787, docs:4321 });
});
test('headless preparation and alternate ports are explicit', () => {
  const options = parseOptions(['--no-open','--no-desktop','--skip-build','--port-offset','10000','--data-root','demo space'], '/project');
  assert.equal(options.noOpen, true); assert.equal(options.noDesktop, true); assert.equal(options.skipBuild, true);
  assert.equal(options.dataRoot, path.resolve('demo space'));
  assert.deepEqual(demoPorts(options.portOffset), { knot:13000, personal:18788, provider:18787, docs:14321 });
});
test('invalid arguments cannot silently select another service or data directory', () => {
  for (const args of [['--port-offset','-1'],['--port-offset','1e3'],['--port-offset','56748'],['--data-root'],['--data-root','--no-open'],['--unknown']]) assert.throws(() => parseOptions(args), /option|offset|directory|value/i);
  assert.equal(parseOptions(['--port-offset','56747']).portOffset,56747);
  assert.equal(parseOptions(['--help']).help,true);
});
