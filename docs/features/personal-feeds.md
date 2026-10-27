# Personal reading feeds

The `tangled-personal` feeds commands keep subscriptions, RSS/Atom imports, and reading state in signed generic Tangled records. All records use `cap` visibility. Private capability gating does not imply encryption from the local host operator. There is no subscription fetch, scheduler, source write, or HTML execution in this version.

Run a JSON request with:

```sh
cargo run -p tangled-personal -- --data ./personal feeds < request.json
```

The reusable API is `feeds::execute(&mut Store, serde_json::Value) -> Result<Value, String>`. Responses contain complete signed envelopes where records are returned; content lives under `body`, and stable references are under `uri`.

## Commands

```json
{"action":"subscribe","url":"https://example.org/feed.xml","title":"Example","tags":["rust"]}
```

`subscribe` canonicalizes an HTTP(S) URL and creates or updates its subscription. `title` and `tags` are optional; omitted fields and third-party body extensions survive updates. An identical request returns the current record without a new version. URLs cannot contain credentials, fragments, whitespace, or exceed 8192 bytes. Tags are sorted and deduplicated, with at most 100 tags of 1–128 bytes each.

```json
{"action":"list"}
{"action":"unsubscribe","url":"https://example.org/feed.xml"}
{"action":"opml"}
```

`list` returns `subscriptions`. `unsubscribe` writes a signed tombstone; ingested entries and reading history remain available. Resubscribing restores the same subscription URI. `opml` returns an escaped OPML 2.0 document in `opml`, suitable for saving as a file. XML-invalid title characters are replaced with U+FFFD in the exported document; the stored title is preserved. OPML import and per-feed folders are not implemented.

```json
{"action":"ingest","url":"https://example.org/feed.xml","xml":"<rss version=\"2.0\"><channel><item><guid>one</guid><title>Hello</title><link>https://example.org/one</link><description>Article text</description></item></channel></rss>"}
```

`ingest` requires an active subscription and a supplied UTF-8 XML string. It parses RSS 2.0 `channel/item` and Atom `feed/entry` with quick-xml. It returns `imported`, `skipped`, and newly created signed `items`. No URL is fetched. The exact supplied XML is saved as an `application/xml` blob, and each imported item references it in `provenance.blob` with format and source URL.

RSS `guid`, `link`, `title`, `content:encoded` (preferred over `description`), and `pubDate` are retained. Atom `id`, alternate `link`, `title`, `content` (preferred over `summary`), `published`, and `updated` are retained. Dates remain source strings, including missing values as empty strings. RSS bodies and Atom HTML content remain untrusted inert strings. Atom XHTML content is flattened to text in document order; the exact original markup remains recoverable from the XML blob. Other source fields remain available in that evidence blob.

An entry key is BLAKE3 of the compact JSON array `[canonicalSourceUrl, sourceId]`. Identity uses RSS guid or Atom id, falls back to link, then to a normalized-content hash. Repeated source IDs within a feed are skipped; identical IDs in different feeds remain distinct. The first imported version is retained when an upstream entry changes. Missing source IDs and links cannot guarantee deduplication across edited content. Malformed XML and validation failures are detected before any write. Repository failures during sequential writes can leave a partial batch; retrying safely skips entries already stored.

```json
{"action":"queue","unread":true,"starred":true,"tags":["later"],"url":"https://example.org/feed.xml","limit":50}
{"action":"mark","item":"tangled://did:sol:OWNER/pub.tangled.feedItem/KEY","read":true,"starred":true,"tags":["later"]}
{"action":"mark","item":"tangled://did:sol:OWNER/pub.tangled.feedItem/KEY","read":false,"starred":false}
{"action":"digest","unread":true,"limit":20}
```

`queue` returns signed `items` with derived `reading` state and `effectiveTags`. It defaults to unread entries, newest local arrival first, with a default limit of 100 and maximum 1000. `unread:false` includes both read and unread items. `starred` optionally filters by the requested boolean. All requested tags must match the union of item reading tags and current subscription tags. `url` optionally selects a source. `mark` requires an existing item URI and at least one of `read`, `starred`, or `tags`; it changes only supplied fields and preserves extensions. `digest` uses the same filters and returns `count`, `items`, and a local plain-text title/link digest in `text`.

## Interoperability and bounds

The extensible v1 body schemas are [feedSubscription](../spec/schemas/pub.tangled.feedSubscription.json), [feedItem](../spec/schemas/pub.tangled.feedItem.json), and [readingState](../spec/schemas/pub.tangled.readingState.json). Subscription keys are BLAKE3 of compact JSON `[canonicalSourceUrl]`. A reading-state key equals its feed-item key. Third-party clients should use those canonical keys for a single current state per item. Missing state means unread and unstarred. Source identity never becomes a fabricated native actor. Envelope `ts` and exact `cid` provide local arrival and evidence versions.

XML input is limited to 2 MiB, 1000 entries, 20,000 elements, depth 64, and 8 MiB of aggregate retained decoded text including descendant copies. Bodies are limited to 256 KiB; titles to 4096 bytes; IDs and links to 8192 bytes; source timestamps to 1024 bytes. DTDs, custom entities, multiple roots, malformed attributes/tags, and truncated documents are rejected. Only predefined and numeric XML references are accepted. HTML is never fetched, evaluated, or rendered by the CLI. Display clients must sanitize HTML and validate source links before opening them. RSS 1.0/RDF, full namespace validation, relative URL resolution, media downloads, full-text extraction, and upstream revision tracking are outside this first version.

A future fetch adapter should explicitly accept a selected subscription, use bounded HTTP(S) without ambient credentials, enforce redirect/time/byte limits, and supply the resulting UTF-8 XML to `ingest`. It should validate network destinations and redirects, and record response metadata as provenance extensions. Automatic refresh is deliberately not part of this implementation.
