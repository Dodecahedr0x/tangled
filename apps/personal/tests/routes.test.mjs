import test from 'node:test';
import assert from 'node:assert/strict';
import { parseRoute, formatRoute, recordLink } from '../web/routes.js';
const uri = `tangled://did:sol:${'1'.repeat(32)}/pub.example.fieldNotes/river-1`;
test('record route round-trips stable URIs and strips unknown parameters', () => {
  const hash = formatRoute('library', { uri });
  assert.deepEqual(parseRoute(hash), { view: 'library', selection: { uri }, error: '' });
  assert.deepEqual(parseRoute(hash + '&session=private&vendor=unknown').selection, { uri });
  assert.equal(recordLink('http://127.0.0.1:8788/#session=private', 'library', uri), `http://127.0.0.1:8788/${hash}`);
});
test('safe defaults reject inherited names, malformed escapes and foreign web URLs', () => {
  for (const hash of ['', '#constructor', '#__proto__']) assert.equal(parseRoute(hash).view, 'today');
  for (const item of ['https://example.test/', 'tangled://did:sol:x/pub.example.note/a', uri + '?query=private', uri + '/more']) {
    const route = parseRoute('#library?item=' + encodeURIComponent(item));
    assert.deepEqual(route.selection, {}); assert.match(route.error, /record link/i);
  }
  assert.match(parseRoute('#notes?item=%E0%A4%A').error, /record link/i);
});
test('note compose modes are explicit and cannot overwrite linked record selection', () => {
  assert.deepEqual(parseRoute('#notes?mode=inbox').selection, { mode: 'inbox' });
  assert.deepEqual(parseRoute(formatRoute('notes', { uri, mode: 'draft' })).selection, { uri });
  assert.deepEqual(parseRoute('#publish?mode=inbox').selection, {});
  assert.deepEqual(parseRoute('#notes?mode=constructor').selection, {});
});
test('route formatting refuses invalid targets and credentials never enter record links', () => {
  assert.throws(() => formatRoute('constructor'), /view/i);
  assert.throws(() => formatRoute('library', { uri: 'javascript:alert(1)' }), /record/i);
  assert.throws(() => recordLink('https://user:secret@example.test/path?session=secret#session=token', 'library', uri), /origin|credential/i);
  const link = recordLink('http://127.0.0.1:8788/?session=secret&other=ok#session=token', 'library', uri);
  assert.equal(new URL(link).search, ''); assert.doesNotMatch(link, /secret|token|session=/);
});
