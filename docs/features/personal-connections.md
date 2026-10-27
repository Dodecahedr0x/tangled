# Connected-source organization

`connections` is a private, offline view over supplied source activity. It writes ordinary owner-signed, capability-gated `pub.tangled.sourceItem`, `pub.tangled.sourceOverlay`, and `pub.tangled.sourceContact` records and content-addressed blobs. The owner signs the projection; the preserved native sender is never presented as a Tangled author. Capability gating does not imply encryption from the local host operator.

Run requests through the personal command interface:

```sh
cargo run -p tangled-personal -- --data .data/personal connections < request.json
```

## Import existing recovery activity

The `activity` array accepts the committed `tangled-import` recovery `activity.json` shape (`ArchiveObject` containing `ImportedObject`). It accepts arbitrary source names; the recovery tool currently emits Telegram, WhatsApp, and Twitter activity. Construct the request without including recovery keys:

```sh
jq '{action:"ingest", activity:.}' recovered/activity.json > request.json
```

Example entry:

```json
{"action":"ingest","activity":[{"key":"archive-key","source":"telegram","account":"native-owner-id","object":{"id":"42","kind":"message","conversation":"room-7","sender":"native-alice","timestamp":"2026-01-01T10:00:00Z","text":"Hello","attachments":["native-file-reference"],"data":{"source_revision":17,"vendor":{"kept":true}}}}]}
```

The response is `{ "items": [signedRecord, ...] }`. Save each returned `rkey` for subsequent actions. The identity key hashes the JSON tuple of source, account, kind, conversation (including null), and native ID. Equal native IDs in different sources, accounts, kinds, or conversations stay separate. Source-wide items should consistently use a null conversation. Each complete supplied entry is retained in `imported` and in its revision entry, including unknown fields; the exact source/account/sender/timestamp/data remain available. Attachment strings stay source references and are not treated as local blobs or downloaded.

Every revision has an `originalBlob` containing a JSON serialization of the supplied entry. This preserves the parsed entry, not whitespace from the enclosing activity file. Optionally add `original:{"name":"export.txt","mime":"text/plain","bytes":[104,105]}` to an entry to preserve exact supplied source bytes as a separate `sourceOriginalBlob`. Retrieve those bytes with `original` and `sourceOriginal:true`. The module never opens the recovery directory, `recovered-account.json`, archive databases, sessions, or connector credentials. Recognized credential field names in supplied JSON cause rejection instead of copying. Supply only noncredential activity: arbitrary secrets embedded in text or raw bytes cannot be identified automatically.

Batches are bounded to 1000 entries / 8 MiB serialized JSON, and optional original byte arrays to 4 MiB each. All entries are validated before writes; storage failures during a batch are not transactional. Retrying identical entries is idempotent and creates no new record or blob.

## Revisions and deleted source items

An identical supplied entry keeps the existing record CID. Distinct entries append retained revisions. A larger nonnegative integer `revisionOrder` on the entry, or `object.data.source_revision`, selects the current revision automatically when both revisions have comparable numeric values. Callers must keep units consistent and supply truthful revision order. Older/equal revisions remain retained without replacing current data. Other native revision fields, timestamps, and opaque IDs are preserved but their order is not guessed. Review ambiguous changes with `history`, then explicitly choose with `select-revision`. Unknown app fields on existing source records survive ingest and selection.

`object.deleted:true` on a selected revision marks a deleted source view. Deleted entries stay available via `get`, `history`, and `list` with `includeDeleted:true`. They are hidden from the default list. This is a source assertion, not a source delete operation. Selection can explicitly restore an older view.

```json
{"action":"get","rkey":"ITEM_KEY"}
{"action":"history","rkey":"ITEM_KEY"}
{"action":"select-revision","rkey":"ITEM_KEY","index":1}
{"action":"original","rkey":"ITEM_KEY","index":0}
{"action":"original","rkey":"ITEM_KEY","index":0,"sourceOriginal":true}
```

## Private organization and search

`list` and `search` return signed item records, with an optional derived `overlay` envelope. Exact source, account, conversation, sender, contact URI, bookmark, and tag filters combine; `query` performs case-insensitive substring search over current text and the local annotation. Empty queries are omitted. Results use the store's stable order, not native chronology.

```json
{"action":"list","source":"telegram","account":"native-owner-id","conversation":"room-7"}
{"action":"search","query":"hello","sender":"native-alice"}
{"action":"list","bookmarked":true,"tag":"work","includeDeleted":true}
{"action":"overlay","rkey":"ITEM_KEY","patch":{"bookmarked":true,"note":"Follow up","tags":["work"],"otherApp":{"color":"blue"}}}
```

Overlays reference stable source item URIs. Patch operations preserve unmentioned extension fields and replace supplied fields. `target` and `reply` cannot be patched. Tags, notes, and bookmarks do not rewrite source data.

Contacts use a separate source/account/native sender identity. Set an alias and groups; identical aliases or groups across services do not merge authors. `contacts` supports source/account/sender/group filters. Use a returned contact's URI in the item `contact` filter.

```json
{"action":"contact","source":"telegram","account":"native-owner-id","sender":"native-alice","patch":{"alias":"Alice","groups":["friends"],"otherApp":{"color":"blue"}}}
{"action":"contacts","group":"friends"}
{"action":"list","contact":"tangled://did:OWNER/pub.tangled.sourceContact/CONTACT_KEY"}
```

## Reviewed reply intent

A reply preview stores a private draft in the item's overlay with explicit source, sending account, recipient, native conversation/message/kind, text, and exact source evidence CID. Account and recipient must match the selected item's account and native sender. This first version supports replying to that sender only; group/broadcast recipient semantics need a source-specific adapter.

```json
{"action":"reply-preview","rkey":"ITEM_KEY","account":"native-owner-id","recipient":"native-alice","text":"Thanks"}
```

Review the complete returned `body.reply` and signed preview CID. Export requires affirmative confirmation of that exact current preview. Any preview edit or source record revision invalidates an earlier approval.

```json
{"action":"reply-export","rkey":"ITEM_KEY","previewCid":"EXACT_PREVIEW_CID","confirmed":true}
```

The response is an explicit `pub.tangled.replyIntent` with `status:"confirmed"`, `delivery:"not-sent"`, native routing fields, text, source evidence, and approved preview CID. Export is read-only and repeatable. It does not mutate native source activity, create native public posts/chats, enqueue sends, call connectors, or claim delivery. A connector may consume this intent only through its own separately reviewed send workflow. Compatibility is validated against the committed recovery JSON structure and local fixtures; no external source APIs or connector send contract were exercised. This generic intent needs a connector adapter before transport.

The three schemas under `docs/spec/schemas/pub.tangled.source*.json` are extensible and app-independent. Additional fields are allowed; unrelated app metadata survives local edits. Source data and organization remain separately portable through the shared signed record/blob export.
