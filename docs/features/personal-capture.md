# Capture and read later

The `capture` workflow saves private signed `pub.tangled.capture` records. Article bodies and attachments use content-addressed blobs; notes use inline plain text. Source metadata stays attached to the saved item. It works offline with supplied content; it does not fetch or extract remote pages itself.

From the repository root, pass JSON on stdin:

```sh
cargo run -p tangled-personal -- --data ./personal capture <<'JSON'
{"action":"save","kind":"article","url":"https://example.org/article","title":"Reading","text":"The full article, saved now.","tags":["later"],"source":{"type":"manual"}}
JSON
```

Open `tools/capture/install.html` in your browser and drag **Save to Tangled** onto the bookmarks bar. On a page, optionally select a passage and click the bookmark. Import the downloaded JSON:

```sh
cargo run -p tangled-personal -- --data ./personal capture < ~/Downloads/tangled-capture.json
```

The helper records the exact current URL and title, selected text in `source.selection`, and visible text from the first article/main element or body. It downloads a local JSON file; no server endpoint, ambient credentials, cookies, or browser storage are used. Capture is limited to 1,048,576 characters in the helper. Some browser or page policies block bookmarklets/downloads; a manually supplied JSON file uses the same interface. The JSON may contain sensitive page text and should be handled accordingly.

## Input and output

Each input is an object with an `action`. Save/get/update mutations return the signed envelope including `body`, `uri`, and `cid`; `list` returns an array. Use the returned `uri` in subsequent requests. URI must identify this local owner and the capture collection.

| Action | Fields | Behavior |
| --- | --- | --- |
| `save` / `capture` | `kind`, optional `title`, `url`, `text`, `mime`, `bytes`, `source`, `tags`, `read`, `starred`, `extensions`, `idempotency_key` | Save or return an existing matching item |
| `list` | Optional `tag`, `read`, `starred` | Filter saved items; tombstones are excluded |
| `get` | `uri` | Return signed item and source metadata |
| `read` / `render` | `uri` | Return `{uri,cid,title,url,text,mime:"text/plain",originalMime,blob}` without changing read status |
| `download` | `uri` | Return `{uri,cid,mime,bytes}` with original bytes as a JSON integer array |
| `update` | `uri`, object `patch` | Update metadata, inline text, article body, and extension fields |
| `tag` | `uri`, string array `tags` | Replace tags, trimming whitespace and removing duplicates |
| `mark-read` / `mark-unread` | `uri` | Explicitly change read state |
| `star` | `uri`, optional boolean `starred` (default true) | Set or clear a star |
| `delete` | `uri` | Write a signed tombstone |

Kinds:

- `bookmark`: requires HTTP(S) `url`. It stores a link without claiming the page content is archived.
- `article`: requires nonempty supplied `text`, saved as a `text/plain` blob; optional original `url`.
- `text`: requires nonempty `text`, saved inline.
- `file`: requires integer `bytes` array (0..255) and `mime`.
- `screenshot`: like file, with an `image/` MIME. The caller supplies screenshot bytes; this workflow does not take screenshots itself.

Without an explicit kind, supplied text means `text`; otherwise it means `bookmark`. Content is limited to 16 MiB per item. Title is limited to 4096 bytes; tags to 128 input entries, each 1..256 bytes after trimming. Source is an extensible object. All captures start unread and unstarred unless explicitly set. The optional `extensions` object adds unknown top-level body fields without overriding known capture fields.

```json
{"action":"save","kind":"bookmark","url":"https://example.org/","title":"Example"}
{"action":"save","kind":"text","text":"A passage to keep","tags":["ideas"]}
{"action":"save","kind":"file","title":"sample.txt","mime":"text/plain","bytes":[72,105]}
{"action":"save","kind":"screenshot","title":"Screen","mime":"image/png","bytes":[137,80,78,71]}
{"action":"list","read":false,"tag":"later"}
{"action":"update","uri":"tangled://OWNER/pub.tangled.capture/KEY","patch":{"title":"Revised","vendor.example":{"reviewed":true}}}
{"action":"read","uri":"tangled://OWNER/pub.tangled.capture/KEY"}
{"action":"star","uri":"tangled://OWNER/pub.tangled.capture/KEY","starred":false}
```

The screenshot example demonstrates byte transport; use an actual complete image for a usable screenshot.

## Identity, revisions, and portability

The stable identity is explicit `idempotency_key` if supplied, otherwise the exact original URL, otherwise a digest of kind/MIME/content. Repeating `save` returns the existing signed item without changing tags, metadata, or its CID. URL captures remain distinct when query strings or fragments differ. Saving an article after bookmarking the same URL returns the bookmark; use an explicit different idempotency key for an additional snapshot. Updates create a signed causal successor and preserve unknown top-level fields. `source` and other nested objects are replaced as whole values when patched. Kind, original URL, identity, timestamp, blob reference, MIME, and size are immutable through metadata patches; article `text` updates replace its canonical blob. Deleted items can be recaptured at the same stable key.

The schema is `docs/spec/schemas/pub.tangled.capture.json`, with unknown fields allowed. Signed Tangled URIs provide stable references and the returned CID identifies exact evidence. CAR export/import includes blobs for offline portability through the shared Store. This is a generic collection, available to other conforming tools.

## Reader and privacy

Reader output is always plain text. HTML-looking text stays literal text; attachments of non-text MIME return a summary and an explicit download action. A client must display reader text with `textContent`, a textarea, or equivalent plain output, never `innerHTML`. SVG/images, scripts, and other original attachment bytes are not embedded or executed. Text files must be valid UTF-8 to read; download still returns the original bytes.

Every saved or edited item uses `cap` visibility. Passing another visibility to save is rejected; editing visibility is rejected. Capability gating is not a claim of encryption from a local or custodial operator. Sharing selected content requires the explicit publication workflow. The helper performs no HTTP fetch, and the Rust workflow never contacts captured URLs. No remote extraction service or paid API is required.

Run validation with `cargo test -p tangled-personal --test capture` and `node --test tools/capture/capture.test.cjs`.
