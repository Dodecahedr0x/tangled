# Does `pub.tangled.chat` specify attachments, search, read receipts, and notifications for a v1 messenger?

## Answer

### Image/blob attachments — (1) specified
Required `circle`; optional `text`, `blob`. Path: `putBlob` → CID in `blob` → `getBlob`. Sealed: AEAD first. No `mime`/`alt`/`media` on chat. Not `pub.tangled.media`/graph. Circles = `circle` envelope.

### Message search — (3) absent protocol; (2) app-local
No search field/method. `loom.query`/`queryBacklinks` public-only; looms MUST NOT hit `sealed`. “Impossible if sealed.”

### Read receipts — (3) absent
No chat receipt field. `sys.circle.mls` `{type:"receipt", epoch, member}` is MLS Welcome, not message-read.

### Notifications — (2) app-local
`POST tangled.sync.notify` `{did, commit}` is a replica/loom head hint (MAY ignore), not push. Members SHOULD `subscribe`/poll.

**e2e:** `e2e.rs` publishes sealed `pub.tangled.chat` ciphertext, not `{text,blob}`. `msg1` `ct:"00"`; `after-grow` epoch-AEAD of `b"hello dual-stack"`.

## Citations
- `spec/schemas/pub.tangled.chat.json:5-14`; `spec/06-data-model.md:132,150` — `circle`/`text`/`blob`
- `spec/12-http-api.md:55-88`; `spec/11-sync.md:33-51` — blobs; notify
- `spec/08-visibility.md:61-65`; `spec/14-looms.md:20-49`; `spec/19-comparison.md:61` — public search only
- `spec/10-circles.md:102,150-152`; `spec/20-use-cases.md:27-35` — MLS receipt; subscribe
- `crates/tangled/tests/e2e.rs:167-174,391-406` — send path

## Confidence: high
Drop if schema adds mime/receipt/search.

## Could not verify
- `blob` raw CID vs media CID: no chat description.
- sealed plaintext = schema JSON: e2e seals raw bytes.
- `tangled.blobref` mime: named in data-model; no schema file.
