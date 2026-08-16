# What blob/media types and size limits does Tangled v1 specify for `pub.tangled.post` media CIDs?

## Answer
Blobs are opaque raw bytes (codec `raw` 0x55); knots MUST NOT infer type. `post.media` is an unbounded array of blob CIDs or `pub.tangled.media` record CIDs. Media records have `blob`, free-string `mime`, optional `alt` — no image/video enum. `putBlob` POSTs `application/octet-stream` → `{cid}`; limit is knot `maxBlob` (MUST ≥1 MiB, SHOULD ≥128 MiB; describe/MockKnot 134217728). Oversize → 413 `BlobTooLarge`. Fetch `GET tangled.sync.getBlob?did=&cid=` → octet-stream; no cap if a live public record references the CID. Video is unspecified (use-cases: images/photos only). Reed: same blob + client mime (e.g. `video/mp4`) + HTML5 player; honor `node.describe.maxBlob`.

## Citations
- spec/06-data-model.md:78-80 — raw blobs, maxBlob floor/should, no type inference
- spec/schemas/pub.tangled.post.json:14-17 — media = blob or media-record CIDs
- spec/schemas/pub.tangled.media.json:6-10 — mime is unconstrained string
- spec/12-http-api.md:55-57,93-95 — getBlob/putBlob octet-stream
- spec/11-sync.md:35-37 — getBlob path and Content-Type
- spec/02-conformance.md:45-49 — maxBlob 134217728
- crates/tangled/src/knot.rs:933-941 — 413 BlobTooLarge vs maxBlob
- crates/tangled/src/http.rs:400-408,482-494 — client get_blob/put_blob
- spec/20-use-cases.md:45,101 — images/photos only
- apps/loomed/core/src/chat.rs:105-116 — Loomed images = putBlob CID, no mime

## Confidence: high
Would drop if a later schema adds a MIME allowlist or a video-specific size/codec rule.

## Could not verify
- Video codec/player: searched spec/, spec/schemas/, crates/tangled for `video` — no hits (only .agent/questions.md Q13).
- `tangled.blobref` MIME: named in spec/06-data-model.md:114; no file in spec/schemas/.
- Media array max length: post schema has no `maxItems`.
