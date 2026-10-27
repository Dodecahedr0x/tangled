---
title: Personal memory
slug: personal-memory
description: Search authorized records and keep portable saved searches, collections and annotations.
order: 10
---

# Searchable personal memory

`memory::execute(&mut Store, input)` searches the authorized current signed records in a local Tangled repository, including collections created by other applications. It returns each source record's stable `uri` and current exact-version `cid`, a title, a UTF-8-safe snippet and a deterministic score. Imports remain their original records; search creates no synthetic users or activity. Tombstones are excluded by Store. No network request occurs.

All actions accept a JSON object. The personal command dispatcher supplies this object to the memory workflow. For example:

```json
{"action":"search","query":"café budget","collection":"pub.tangled.article","tag":"work","limit":20,"offset":0}
```

Whitespace separates terms; all terms must occur, case-insensitively, as substrings of the source text or its personal annotations. Title matches receive a bonus of three per term; other score contributions count occurrences. Ties sort by URI. Empty queries browse matching records. `total` reports matches before pagination. `limit` defaults to 20 (1–1000); `offset` defaults to 0. Invalid filter types and pagination return errors.

Optional filters combine with AND:

- `collection`: exact protocol collection, including unknown third-party names.
- `source`: exact author DID, string `body.source`, or `kind`, `id`, `url`, `uri` or `provider` in a source object.
- `tag`: case-insensitive exact tag from `body.tags` or a personal annotation targeting that record.
- `group`: the rkey of a personal `pub.tangled.collection`; membership follows stable URI, even when its source has a newer CID.

Search covers string values in these top-level body fields: `title`, `text`, `description`, `summary`, `content`, `name`, `caption`, `quote`, `transcript`, `ocrText`, `notes`, `alt`, plus string tags. It does not serialize arbitrary bodies or index envelope signatures, credential objects, passwords, keys, ciphertext fields, or sealed/encrypted records. Text deliberately placed in a supported human-readable field is searchable: this is a field selection policy, not a secret detector. Collections and saved-search definitions are excluded from source hits; annotations are independently searchable and also enrich their referenced source.

Known content adapters also search `pub.tangled.capture` article blobs when `kind` is `article` and `mime` is exactly `text/plain`, and string `pub.tangled.feedItem` bodies with `bodyType` of `text` or `html`. Article blobs must be valid UTF-8 and at most 256 KiB; larger blobs are omitted. Feed content is limited to 256 KiB. HTML feeds contribute plain text after tags, attributes, and script/style content are discarded. Generic top-level text is bounded to 256 KiB per source. The adapter never follows arbitrary blob references or nested credential objects. Snippets remain plain text data; clients must render them with `textContent`, never as HTML. Evidence identifies the signed capture or feed-item envelope, whose body identifies its content.

## Saved searches

```json
{"action":"save-search","rkey":"work-budget","name":"Work budget","query":"budget","tag":"work"}
```

`save-search` creates or updates `pub.tangled.savedSearch`. It accepts all search fields, plus required `rkey` and `name` (name may be inherited on edit). Omitted fields retain their prior values; set optional string filters to `""` to clear them. `query` defaults to `""` when creating. Use `run-search` with `rkey` to recompute against current records, `list-searches` to return `{"records":[...]}`, and `delete-search` with `rkey` to write a tombstone. Search results themselves are never persisted as cached truth.

## Collections and annotations

```json
{"action":"put-collection","rkey":"project","name":"Project","items":[{"uri":"tangled://did:sol:OWNER/pub.tangled.article/abc","cid":"SOURCE_CID"}]}
```

`put-collection` creates or updates a named `pub.tangled.collection`. Fields are `name`, optional `description`, `tags`, and `items` (defaults to `[]` on creation). Each item requires a Tangled record URI and may pin a CID as evidence. Use `list-collections` to return `{"records":[...]}` or `delete-collection` with `rkey` to tombstone it. Deleting a collection leaves referenced records intact.

```json
{"action":"annotate","rkey":"budget-question","text":"Ask about the budget","target":{"uri":"tangled://did:sol:OWNER/pub.tangled.article/abc","cid":"SOURCE_CID"},"tags":["work"]}
```

`annotate` creates or edits a `pub.tangled.annotation` with `text` (up to 32768 characters), required `target`, optional `quote`, and optional `tags`. Target requires a Tangled record URI; CID is optional evidence. Tags and text remain attached to the stable URI when the source changes; a pinned CID documents the version originally annotated. References may point to records unavailable locally; missing references produce no source hit until available. Use `delete-annotation` with `rkey` to tombstone it.

Writes return the signed envelope and use `vis: cap`. Existing unknown body fields survive edits; extension fields inside an existing target object also survive. Annotations never modify the source. All metadata uses documented extensible generic schemas in `docs/spec/schemas/`; the annotation profile adds fields to the existing normative annotation format.

## First-version limits

Search scans authorized records in memory on each request. It has no persistent index, semantic embeddings, fuzzy matching, stemming, phrase language, Unicode normalization, accent folding, OCR engine, transcription dependency, generic blob extraction, or automatic remote fetching. Existing supplied `ocrText`/`transcript` text is searchable. Snippets show up to 240 Unicode characters around the earliest matched term; additional terms may fall outside that window. Nested rich-text objects and application-specific fields outside the listed fields require a future adapter. Capability gating is not encryption from a custodial/local host. Search results can include private plaintext, and callers must keep them within the same authorization boundary.
