const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(`${__dirname}/capture.js`, 'utf8');

function browser(url = 'https://example.test/story') {
  let downloaded;
  let clicked = 0;
  const anchor = { href: '', download: '', click() { clicked++; }, remove() {} };
  const document = {
    title: 'Saved title',
    body: { innerText: 'Fallback visible text', appendChild() {} },
    querySelector() { return { innerText: 'Article text\n<script>literal text</script>' }; },
    createElement() { return anchor; },
    getElementById() { return null; },
  };
  const window = { location: { href: url }, getSelection() { return { toString() { return 'Selected passage'; } }; } };
  const context = {window, document, Blob, URL: { createObjectURL(blob) { downloaded = blob; return 'blob:local'; }, revokeObjectURL() {} }, setTimeout(fn) { fn(); }, alert() {} };
  vm.runInNewContext(source, context);
  return {context, anchor, get downloaded() { return downloaded; }, get clicked() { return clicked; }};
}

test('browser helper downloads supplied article and source metadata as JSON', async () => {
  const page = browser();
  page.context.window.captureCurrentPage();
  const payload = JSON.parse(await page.downloaded.text());
  assert.equal(page.clicked, 1);
  assert.equal(page.anchor.download, 'tangled-capture.json');
  assert.equal(payload.action, 'save');
  assert.equal(payload.url, 'https://example.test/story');
  assert.equal(payload.title, 'Saved title');
  assert.equal(payload.kind, 'article');
  assert.equal(payload.text, 'Article text\n<script>literal text</script>');
  assert.equal(payload.source.selection, 'Selected passage');
});

test('helper rejects non-web page schemes before downloading', () => {
  const page = browser('javascript:bad()');
  page.context.window.captureCurrentPage();
  assert.equal(page.clicked, 0);
  assert.equal(page.downloaded, undefined);
});
