---
title: Personal publishing
slug: personal-publish
description: Deliberately export selected content as readable HTML, RSS and signed public publications.
order: 16
---

# Publish and share selected content

The `publish` workflow creates readable static HTML and RSS from explicitly selected local signed records. Someone can open the exported `index.html`, read the content, and download selected attachments without a Tangled account. Send the directory as a ZIP or host its files yourself. This command does not deploy a site, create a live guest-link service, send messages, or modify the source application's data.

All durable publications use the ordinary extensible `pub.tangled.publication` collection. Other clients can read its plain `items` array and generated content-addressed `html`/`rss` blobs using generic record and blob APIs. Source content may come from any collection, including an unknown third-party client; there is no application-private database or schema whitelist.

```sh
cargo run -p tangled-personal -- --data .data/personal publish <<'JSON'
{"action":"export","confirmPublish":true,"title":"A few things to read","records":[{"collection":"pub.tangled.note","rkey":"CHOSEN_KEY"}],"outputDir":"./shared-reading"}
JSON
```

Use an actual local record key. `outputDir` must be a **new directory** with an existing parent. Existing files/directories are never overwritten. The new directory contains `index.html`, `feed.xml`, and explicitly selected original attachments under `attachments/`. The exported HTML needs no server, JavaScript, external assets, or login. A failed file write removes the newly created partial export directory.

## Actions and selections

Each action returns `{html,rss,count,files}`. `files` is an array of `{path,mediaType,text}` for generated documents, or `{path,mediaType,bytes}` for selected attachments; `bytes` is an integer array containing the original bytes. This makes file output optional and lets another program choose where to save files. `export` with `outputDir` also returns that path. `render` and `export` create no signed records.

| Action | Fields | Result |
| --- | --- | --- |
| `render` | Explicit selection, optional `title`, `description`, `siteUrl`, `confirmPublish` | Generated HTML/RSS and selected files in JSON |
| `export` | Same, optional `outputDir` | Same output; optionally create a complete static sharing directory |
| `publish` | Same selection, required stable `key` and `confirmPublish:true` | Same output plus the public signed `publication` envelope |

Choose exactly one selection form:

```json
{"action":"render","records":[{"collection":"pub.example.story","rkey":"one"},{"uri":"tangled://OWNER/pub.example.story/two"}]}
{"action":"render","source":{"collection":"pub.example.story","rkey":"one"}}
{"action":"render","collection":"pub.example.story","rkey":"one"}
{"action":"render","uri":"tangled://OWNER/pub.example.story/one"}
{"action":"export","collection":"pub.example.story","confirmPublish":true}
```

Replace `OWNER` with this local store's DID. Only local-owner records are selected; remote references are not fetched. An entire `collection` is an explicit selection of all current live records in that collection. Empty selections, duplicate records, mixed selection forms, more than 256 records, or missing records fail. Tombstones are excluded. If any selected record is private, the **entire** request fails without `confirmPublish:true`, including render/static export. Public source records can be rendered/exported without that flag. Signing any public publication always requires it.

Only top-level plain string `title`, `text`, and `description` are copied from selected bodies. Missing fields become `Untitled`/empty text. Source URLs, exact source envelope CIDs, source Tangled URIs, provenance, tags, arbitrary extension objects, embedded media, and unselected attachments are not copied. Even a source's canonical `blob` is ignored until individually selected. To include a web link, a UTF-8 blob as body text, or original attachments, explicitly select them:

```json
{
  "action":"export",
  "confirmPublish":true,
  "title":"Selected reading",
  "siteUrl":"https://example.org/reading/",
  "records":[{
    "collection":"pub.example.story",
    "rkey":"one",
    "link":"https://example.org/original",
    "textBlob":"SELECTED_TEXT_BLOB_CID",
    "attachments":[{"cid":"SELECTED_ORIGINAL_BLOB_CID","name":"Original diagram"}]
  }]
}
```

`textBlob` replaces the inline `text` with exactly the explicitly chosen UTF-8 bytes interpreted as plain text. Attachments preserve original bytes; they use content-derived `.bin` filenames, download links, and `application/octet-stream` in the export rather than embedding potentially executable HTML/SVG. Each item allows at most 32 attachments. Selected content has a conservative 16 MiB aggregate limit, including decoded text and attachment names. Titles/names allow 4096 bytes. Links allow 8192 bytes and only HTTP(S) URLs with a host and no credentials or control characters. No HTTP request is made.

HTML escapes all content and has a restrictive content security policy. XML text/attributes are escaped and illegal XML control characters are replaced. RSS descriptions are escaped again for readers that interpret descriptions as HTML after XML decoding. Original attachment bytes are downloads and are never rendered inside generated pages. Clients rendering publication `items` must similarly treat strings as plain text.

Supply `siteUrl` when hosting the export so the RSS channel has its actual public website URL. Without it, the channel link is empty: the files remain useful for offline sharing, but there is no hosted feed address to subscribe to. The RSS GUID derives only from selected copied content, not private source identifiers. Explicit source `link` values become item links. Attachments are available in the static HTML, not RSS enclosures.

## Public signed publication and updates

```json
{"action":"publish","confirmPublish":true,"key":"reading-list","title":"Reading list","records":[{"collection":"pub.example.story","rkey":"one"}]}
{"action":"publish","confirmPublish":true,"key":"reading-list","title":"Revised reading list","records":[{"collection":"pub.example.story","rkey":"one"}]}
```

The explicit `key` gives the publication a stable URI, `tangled://OWNER/pub.tangled.publication/reading-list`. Repeating publish updates that same ordinary signed record with a causal predecessor and new CID. Re-render from the selected sources to update content/title/description/site URL; omitted description/site URL are cleared. Unknown existing public publication extension fields are preserved. A private publication at the same key is rejected instead of silently converting it; choose a new key. The schema is [`pub.tangled.publication.json`](../spec/schemas/pub.tangled.publication.json).

Publication creates a separate public copy and references only generated document blobs and individually selected attachment CIDs. Sources remain unchanged and capability-gated; this is not encryption from a local/custodial operator. Selecting an original attachment for a public publication intentionally makes those exact content-addressed bytes public through the publication. Publishing does not automatically follow later edits, write back to a source, or host/synchronize the repository on a public network. Already shared files and bytes cannot be recalled by editing a record. Keep private full-repository CAR backups separate from these selected-content sharing files.

Validation: `CARGO_TARGET_DIR=target/publish cargo test -p tangled-personal --offline` and focused `cargo clippy -p tangled-personal --offline --test publish -- -D warnings`.
