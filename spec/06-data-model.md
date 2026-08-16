---
title: Data model
slug: data-model
order: 6
---

# Data model

Everything a knot stores is either a **record** (a typed, signed object in the Merkle Search Tree) or a **blob** (opaque bytes addressed by CID). Schemas are records. Capabilities are records. Foreign envelopes are records.

## Record

CDDL: `Record` in `spec/cddl/record.cddl`.

```
Record v1 = {
  v: 1,
  type: cid,              ; schema CID
  collection: text,       ; e.g. "pub.tangled.post"
  rkey: text,             ; unique in (did, collection)
  author: did,            ; "did:sol:<base58>"
  ts: uint,               ; Unix milliseconds
  prev: [* cid],          ; causal parents
  vis: vis,               ; "public" / "cap" / "sealed"
  body: any / ciphertext,
  ? signedBy: bstr .size 32,
  ? proof: [* cid],       ; capability CIDs
  ? ref: cid,             ; annotation target (record CID)
  ? epoch: uint,          ; circle epoch for sealed application data
  ? circle: text,         ; did:sol:<owner>#<circle-rkey> when circle-scoped
  sig: bstr .size 64
}
```

`type` is the CID of a `sys.schema` record. Implementations MAY also match `collection` as a hint; verification is against `type`.

`rkey` MUST match `[A-Za-z0-9._~:!-]{1,256}`. A timestamp identifier (TID) is RECOMMENDED: 13-character Crockford base32 of `(unix_micros << 10) | entropy10`. Nested wiki slugs for `pub.tangled.page` are specified in [Page slugs](#page-slugs).

`prev` lists CIDs of **same-key** causal parents: records that share this record’s `(author, collection, rkey)`. Empty means a root. Cross-record replies and annotations MUST use envelope `ref` (and MAY repeat the target in the application body as `reply`); they MUST NOT use `prev` for a different rkey. Same-key concurrent writes use the collection’s merge class ([Merge rules](#merge-rules)). For Conflict-free Replicated Data Type (CRDT) collections, the record Content Identifier (CID) is the operation id.

`signedBy` and `proof` are REQUIRED when the signer is a device key. They MUST be absent when the identity key signs.

`ref` is the CID of the record this record annotates or replies to at the envelope layer. Application bodies MAY also carry `reply` (a record CID). Clients assembling threads MUST treat `ref` as authoritative when both are present and MUST reject a public record whose `reply` CID disagrees with `ref`.

`circle` marks a **circle-scoped** record. If `circle` is present:

- `vis` MUST be `sealed`
- the value MUST be `did:sol:<owner>#<circle-rkey>` identifying a `sys.circle` record
- after decrypt, if the application body contains a `circle` field, it MUST equal the envelope value; clients MUST discard a mismatch
- fetch MAY be authorized by `circle/read` on that circle, not only by `repo/read` on this author ([Capabilities](./09-capabilities.md), [Circles](./10-circles.md#membership-grant))

The record CID is BLAKE3 of the **complete** DAG-CBOR including `sig`. The unsigned map (no `sig`) is the signing input as specified in [Cryptography](./05-cryptography.md).

## Visibility

`vis` is defined in [Visibility](./08-visibility.md). Summary:

- `public` — body is plaintext DAG-CBOR of the application value.
- `cap` — body is plaintext; the knot serves it only with a capability.
- `sealed` — body is a `Ciphertext` map.

## Ciphertext

```
Ciphertext = {
  alg: "hpke-record" / "epoch-aead",
  ? enc: bstr,            ; HPKE encapsulating key
  ? recipients: [* { kid: bstr, enc: bstr, ct: bstr }],
  ct: bstr,               ; ciphertext + tag
  ? epoch: uint
}
```

`alg = "hpke-record"` is 1:1 HPKE. `alg = "epoch-aead"` is circle AEAD under `epoch_secret[epoch]`.

## Blobs

A blob is raw bytes. Its CID uses codec `raw` (0x55) and BLAKE3-256. Maximum size is the knot’s `maxBlob` (MUST be at least 1 MiB, SHOULD be at least 128 MiB).

Records reference blobs by CID. Knots MUST NOT infer a blob’s meaning from its bytes. Sealed records MAY store ciphertext as a blob and keep a small `Ciphertext` header in the record with `ct` omitted and `blob: cid` set. For sealed media, the blob bytes MUST be ciphertext, not plaintext. Fetch is `tangled.sync.getBlob` ([Synchronization](./11-sync.md#get-blob)).

## Schemas

Collection `sys.schema`, rkey = TID. Body:

```
Schema = {
  v: 1,
  name: text,             ; "pub.tangled.post"
  fields: any,            ; JSON Schema 2020-12 object describing the body
  ? cddl: text            ; optional CDDL for the body
}
```

The schema’s CID is `type` on instance records. Evolving a schema means publishing a new schema record (new CID). Old records keep the old `type`. Compatibility maps are OPTIONAL application data, not a protocol concern.

Core schemas in `spec/schemas/` are frozen for version 1. Applications add schemas as records; they do not need a registry.

## Core collections

| Collection | rkey | Purpose |
| --- | --- | --- |
| `sys.id` | `self` | DID document |
| `sys.schema` | TID | Schemas |
| `sys.revocation` | capability CID (base32) | Revocations |
| `sys.binding` | TID | Foreign identity bindings |
| `sys.foreign` | TID | Foreign object envelopes |
| `sys.circle` | circle TID | Circle definition |
| `sys.circle.mls` | TID | MLS Welcome/Commit |
| `sys.circle.wrap` | `{epoch}-{member-base58}` | HPKE wraps of epoch secrets |
| `tangled.graph.follow` | target DID (safe) | Follow |
| `tangled.graph.block` | target DID (safe) | Block |
| `tangled.label` | TID | Labels |
| `tangled.blobref` | blob CID (base32) | Optional metadata for a blob |

“Safe” DID rkey: replace `:` with `.` so `did.sol.<base58>`.

A follow record MAY include `knot` (the target’s knot URL) so first-time clients can resolve without a directory. Clients MUST still verify `sys/id` at that URL.

## Example application collections

Informative, fully specified in `spec/schemas/`:

| Collection | Body gist |
| --- | --- |
| `pub.tangled.post` | `text` (≤ 32 KiB), `langs`, optional `reply` (parent record CID), optional `media` CIDs |
| `pub.tangled.article` | `title`, optional `text` preview, optional canonical `blob`, `langs`, optional `media` CIDs |
| `pub.tangled.annotation` | `text`, optional `quote` span, optional `media`; envelope `ref` MUST be the target record CID |
| `pub.tangled.media` | `blob`, `alt`, `mime` |
| `pub.tangled.album` | `title`, `items` (CIDs of `pub.tangled.media` records) |
| `pub.tangled.page` | wiki page CRDT: incremental `ops` (Last-Writer-Wins (LWW) `title`/`blob`, Replicated Growable Array (RGA) `text`) or a signed **compact** (snapshot + frontier); rkey is the slug encoded as in [Page slugs](#page-slugs) |
| `pub.tangled.chat` | sealed circle message: `text` or blob; body `circle` MUST equal envelope `circle` |

These exist so a substrate is not a secret microblog. Conforming knots MUST sync them like any collection. Conforming clients need not implement their UI. When `pub.tangled.article.blob` is set, clients MUST treat that blob as the canonical body; `text` is a preview.

## Merge rules

Records that share `(author, collection, rkey)` form a causal DAG via `prev`. A replica’s **apply set** `S` for that key is every such record the replica has verified (signature, CID, schema). Two replicas that have the same `S` MUST produce the same **live value**, independent of the order they first received the records. For `pub.tangled.page`, after a valid compact `C`, materialization MAY use `C`’s snapshot plus records not in `I(C)` ([Compaction](#compaction)) only after the client has fetched and verified every CID in `I(C)`.

The Merkle Search Tree (MST) key `collection/rkey` holds **one** CID (the writer’s tip). That tip is not always the live value. A repository Content Addressable aRchive (CAR) MUST include every record block in the `prev`-closure of each MST value so a pull of one head includes that writer’s DAG, except that **compact-GC** MAY omit folded page-op blocks as specified in [Compaction](#compaction). Concurrent heads that exist only on another replica are learned by pulling that replica, or by a later record whose `prev` lists both heads.

### Collection merge classes

| Collection | Merge class | Notes |
| --- | --- | --- |
| `pub.tangled.page` | CRDT | RGA on `text`; LWW-register on `title` and `blob`. Compaction is specified below. |
| `pub.tangled.post` | Last-ts-wins | Whole-body. Concurrent edits do not field-merge. |
| `pub.tangled.article` | Last-ts-wins | Same. `blob`, when set, is the canonical body. |
| `pub.tangled.annotation` | Last-ts-wins | New rkey per annotation is typical. |
| `pub.tangled.chat` | Last-ts-wins | New rkey per message is typical. |
| `pub.tangled.media` | Last-ts-wins | |
| `pub.tangled.album` | Last-ts-wins | `items` is not an Observed-Remove Set (OR-Set) in v1. |
| `tangled.graph.follow` | Last-ts-wins | rkey is the subject; set membership is the key. |
| `tangled.graph.block` | Last-ts-wins | Same. |
| `tangled.label` | Last-ts-wins | New rkey per label is typical. |
| `sys.*` | Last-ts-wins | |

Applications that add a collaborative body (shared document, shared dataset row edited in place) MUST declare a CRDT in their `sys.schema` and MUST specify ops as this chapter does for pages. Other application collections default to Last-ts-wins.

Clients MUST NOT substitute an unspecified CRDT (including Automerge or Yjs as a black box) for a Last-ts-wins collection. They MUST NOT substitute Last-ts-wins for `pub.tangled.page`.

### Causal heads

These definitions apply to every merge class:

1. Record B **supersedes** record A when they share `(author, collection, rkey)` and A’s CID appears in B’s `prev`, or in the `prev` of a same-key ancestor of B.
2. A **causal head** is a same-key record in `S` that is not superseded.
3. A **merge record** is a same-key record whose `prev` lists two or more causal heads. A client that has seen concurrent heads and writes again MUST include every such head it considered in `prev`.

### Last-ts-wins among causal heads

For Last-ts-wins collections:

1. The **live** record is the causal head with the greatest `ts`. If `ts` is equal, the lexicographically greatest record CID wins (CID compared as multibase base32 text).
2. If no merge record exists, step 1 picks one concurrent head. The loser’s bytes remain as a block if some other CID still links them; they are not the live value.
3. A delete (`body` `null`) is an ordinary record under this rule: its `prev` SHOULD list every causal head being deleted. The live value is `null` when the winning head is a tombstone.

### Page CRDT

`pub.tangled.page` is a CRDT. Each non-tombstone same-key record is either one signed incremental operation or one signed **compact**. **Op-id** is that record’s CID. Ops are not gossiped unsigned.

CDDL: `PageBody` in `spec/cddl/page.cddl`. JSON Schema: `spec/schemas/pub.tangled.page.json`. Concurrent-edit vector: `spec/vectors/page-crdt-concurrent.json`. Compaction vector: `spec/vectors/page-crdt-compact.json`.

An incremental body is a list of ops. There is no parallel authoritative `title` / `text` field on an incremental record. A compact carries a **snapshot** of the live RGA and LWW registers at a **frontier**; that snapshot is not a second document format. Clients still materialize.

```
PageBody = ops-body / compact-body

ops-body = { ops: [* page-op] }

atom-id = { op: cid, i: uint } / { i: uint }

page-op = {
  op: "ins",
  after: atom-id / null,
  run: tstr
} / {
  op: "del",
  ids: [+ atom-id]
} / {
  op: "lww",
  path: "title" / "blob",
  value: tstr / null
}
```

`atom-id` with only `i` means atom `(this-record-CID, i)`. `after` `null` means the virtual root. `run` MUST be well-formed UTF-8 and MUST contain at least one Unicode scalar value. `lww` `path` `"title"`: `value` is text (empty allowed, `null` MUST NOT). `lww` `path` `"blob"`: `value` is a CID string, or `null` to clear.

**Atom ids.** For record CID `C`, walk `ops` in array order. Each `ins` consumes the next indices: the `k`-th Unicode scalar (code point) in `run` is atom `{op: C, i: n+k}` where `n` is the number of scalars already created by earlier `ins` ops in this record. `del`, `lww`, and compact bodies do not consume indices. Combining marks are separate scalars. Implementations MUST NOT apply Unicode Normalization Form Canonical Composition (NFC) to `run` before splitting.

**`after`.** MUST be `null`, or an atom that exists in the RGA implied by this record’s `prev`-closure (including atoms listed in a compact snapshot in that closure), or an earlier atom index of this same record (`i` strictly less than the first new index of this `ins`). Invalid `ins` ops MUST be ignored (the run is not created). Invalid `del` ids MUST be ignored.

**Apply (`S`).** If every causal head in `S` has `body` `null`, the live value is `null` (page deleted). Otherwise let `R` be the non-tombstone records in `S`. Partition `R` into compact records and incremental records. For each compact `C` in `R`, the **included set** `I(C)` is the `included` CID list on `C` (the `prev`-closure of the frontier, excluding `C`). An incremental record `X` is **covered** by `C` when `X`’s CID is in `I(C)`. A client MUST reject `C` if any CID in `I(C)` cannot be fetched and verified as a signed page op, or if Apply(`I(C)`) does not equal `C`’s snapshot (`text` / `title` / `blob` and the RGA atom table).

1. Start from an empty RGA and LWW registers `title = ""`, `blob = null` with no candidates.
2. For every compact `C` in `R` (any order): **union** `C`’s snapshot. Each snapshot atom is identified by its original atom-id. If the atom is new, insert it with the listed parent, scalar, and `del` flag. If it already exists, `del` becomes the logical OR of the two flags; parent and scalar MUST match or `C` is invalid. Each snapshot LWW register that is present is a candidate `(ts, cid, value)` as stored in the snapshot (the compact record’s own `ts` MUST NOT replace those).
3. Let `U` be incremental records in `R` that are not covered by any compact in `R`. Create every atom from every `ins` in `U` (any record order). An `ins` after `A` makes the first new atom a child of `A` (or of the virtual root). Each later scalar in that `run` is a child of the previous scalar in the run, so a run stays contiguous. If an atom-id already exists, the `ins` is a no-op for that scalar.
4. Then apply every `del` in `U`: set `del = true` on each listed atom that exists. Deleted atoms remain in the tree (they still parent later inserts).
5. Each `lww` op in `U` is a candidate `(ts, cid, value)` using the **record** `ts` and CID. The winning candidate for a path is the greatest `ts`; if `ts` is equal, the lexicographically greatest record CID (multibase base32 text). Several `lww` ops for one path in one record: the last in `ops` is that record’s candidate. Snapshot candidates from step 2 compete in the same order.
6. **Materialize `text`:** depth-first from the virtual root. Children of a parent are ordered by **atom-id descending**: compare `op` as multibase base32 text, then `i` as an integer. Emit a scalar if `del` is false, then visit that atom’s children. Tombstoned atoms emit nothing but are still visited.

Steps 3 then 4 then 5 are required so create and delete commute. Union of snapshots is commutative. Clients MUST NOT take the MST tip’s body as the page text. Clients that materialize pages MUST implement compact bodies. They MUST NOT treat a compact body as an empty `ops` list. On an incremental body, clients MUST ignore ops whose `op` is not `"ins"`, `"del"`, or `"lww"`.

**Bounds.** Visible `text` MUST NOT exceed `PAGE_TEXT_MAX_SCALARS` (1 048 576) Unicode scalars. The RGA, including tombstoned atoms, MUST NOT exceed `PAGE_RGA_MAX_ATOMS` (4 194 304). `title` MUST NOT exceed 512 Unicode scalars. If `S` would exceed a bound, clients MUST refuse to materialize and MUST treat the page as invalid. They MUST NOT apply a prefix of `S`. After a valid compact, materialization MAY use that compact plus records not in `I(C)` rather than the entire history. The atom caps remain a Denial of Service (DoS) bound on the live structure (compaction does not drop tombstoned atoms).

**Knot write.** For a CRDT collection, `tangled.repo.putRecord` MUST fail with `ConcurrentHeads` when the key already has an MST value `H`, the new record’s `prev`-closure does not include `H`, and `H`’s `prev`-closure does not include the new record. The error body MUST include `head` (the CID of `H`). The writer then fetches `H`, includes it in `prev`, and retries. Knots MUST NOT drop `H` to accept the new record.

When `blob` is the winning LWW value and is not `null`, it is an optional attached blob, not a second canonical body. Live wiki text is the RGA. Collaborative body belongs in `ins`/`del` ops, not in a blob.

### Compaction

A **compact** is a signed same-key `pub.tangled.page` record whose body is `compact-body`. It folds a prefix of the op DAG into a snapshot so replicas MAY materialize from a smaller apply set. Last-ts-wins collections do not compact in version 1.

A **frontier** is the set of same-key record CIDs that the compact claims as the causal heads of that prefix. The **included set** `I(C)` is every record in the `prev`-closure of those CIDs (equivalently, the `prev`-closure of `C` excluding `C`, when `prev` equals the frontier). A compact record MUST permanently list `I(C)` as `included`. A **snapshot** is the RGA atom table and LWW register candidates obtained by applying `I(C)` under [Apply (`S`)](#page-crdt).

```
compact-body = {
  compact: 1,
  frontier: [+ cid],
  included: [* cid],
  snapshot: page-snapshot
}

page-snapshot = {
  atoms: [* snapshot-atom],
  title: lww-snap,
  blob: lww-snap / null
}

lww-snap = {
  value: tstr / null,
  ts: uint,
  cid: cid
}

snapshot-atom = {
  id: { op: cid, i: uint },
  parent: { op: cid, i: uint } / null,
  ch: tstr,
  del: bool
}
```

**Shape.** `compact` MUST be the integer `1`. `frontier` MUST be non-empty and MUST be sorted lexicographically as multibase base32 CID text. `included` MUST be the CID set `I(C)`, sorted the same way. Envelope `prev` MUST be the same set as `frontier` (order in `prev` MAY differ). Snapshot `atoms` MUST be sorted by atom-id ascending (`op` as multibase base32 text, then `i`). Each `id.op` MUST be the creating record’s CID (never omitted). `parent` `null` means the virtual root. `ch` MUST be exactly one Unicode scalar. `title.value` MUST be text, not `null`. `blob` `null` means no `lww` on `blob` was in `I(C)`. A compact MUST NOT create new atom ids. `compact` values other than `1` make the page invalid to materialize.

**Who MAY compact.** Any principal that MAY `putRecord` that key: the identity key, or a device key with `repo/write` covering the page (including a shared-repo collaborator). There is no separate compact capability. Circle `policy` has no compact field. Compaction MUST NOT be presented as a Hybrid Public Key Encryption (HPKE) versus Messaging Layer Security (MLS) choice.

**When.** A writer SHOULD compact only when it can fold at least `PAGE_COMPACT_MIN_OPS` (32) incremental records that are not in `I(K)` for any compact `K` already in the writer’s `prev`-closure. Knots MUST reject a compact with `RateLimited` unless both hold:

- at least `PAGE_COMPACT_MIN_OPS` incremental records in `I(C)` are not in `I(K)` for the nearest compact ancestor `K` of `C` (all of `I(C)` counts when there is no such `K`)
- `C.ts - K.ts >= PAGE_COMPACT_MIN_INTERVAL_MS` (300 000) when such a `K` exists in the knot’s apply set

Those constants are admission control. They are not CRDT validity: a compact that folds fewer ops is algorithmically valid if a replica already has it. The authoring knot MUST retain every block in `I(C)`, so it can always compute the min-ops count.

A writer MUST NOT emit a compact when every causal head in its apply set is a tombstone (that would resurrect a deleted page). A writer SHOULD set `frontier` to every causal head in its apply set so the compact is not immediately stale.

**Visibility.** `vis`, `circle`, and sealed `epoch` on a compact MUST match the records in `I(C)` when those records are present. Mixed `vis` in `I(C)` makes the compact invalid. A sealed or circle-scoped page stays sealed or circle-scoped: the compact body is the plaintext inside the same `Ciphertext` envelope class. Compaction MUST NOT change visibility class.

**Safety (omit / stale / invalid).** A compact MUST NOT drop ops that are in `I(C)`: the snapshot MUST equal Apply(`I(C)`). A client MUST fetch every CID in `I(C)` (`getRepo` / `getBlocks`) and MUST verify each as a signed page op that `C` claims to fold. It MUST replay those ops from the previous frontier (or empty) and MUST reject `C` if the live `text` / `title` / `blob` (and RGA atom table) do not equal the snapshot. A compact is **invalid** when `compact ≠ 1`, `frontier` is empty or unsorted, `included` is unsorted or disagrees with the `prev`-closure of the frontier, `prev` and `frontier` differ as sets, snapshot atoms are unsorted or duplicate, an atom’s `ch` is not one scalar, a parent is missing or cyclic, `vis`/`circle`/`epoch` mismatch folded records, any CID in `I(C)` cannot be fetched or verified, or the snapshot disagrees with Apply(`I(C)`). An invalid compact MUST NOT contribute a snapshot or an included set. If it is a causal head, clients MUST refuse to materialize the page.

A compact that omits an op **concurrent** with its frontier (not in `I(C)`, and `C` not in that op’s `prev`-closure) is **stale** relative to that op, not automatically invalid. A reader that still has extra ops MUST merge them on top of the snapshot by the apply algorithm (`U` includes those ops). Detection:

- Extra op `X` is in `I(C)`: `X` is folded. If the snapshot disagrees with Apply(`I(C)`), the compact is **invalid** (above).
- `C` is in `X`’s `prev`-closure: `X` is after the compact; apply `X` on top.
- Otherwise `X` is concurrent with `C`: the compact is **stale** vs `X`; apply `X` on top. The writer who omitted `X` from the frontier did not include it; readers who have `X` keep it.

**Convergence.** Two replicas that have seen the same set of incremental ops and compact records MUST produce the same live value, independent of compact times and apply order. Union of snapshots plus uncovered incremental ops is deterministic. A replica that compacted early (`C1`) and a replica that compacted later (`C2` with `I(C1) ⊆ I(C2)`) converge once both have `C2` or once both have `C1` plus the extra ops in `I(C2) \ I(C1)`. Concurrent incomparable compacts union by atom-id as in step 2.

**Retention (`I(C)`).** The authoring knot MUST NOT drop blocks in `I(C)` from its store. `getRepo` and `getBlocks` MUST still serve them. Knots MUST keep blocks needed to serve concurrent heads. The compact record `C` MUST remain while it is in the `prev`-closure of any causal head or MST value; sync proves inclusion of `C` by that closure and by the MST inclusion proof of the page key. Knots MUST NOT GC `C` itself in order to drop pre-compact blocks. There is no compact-GC of `I(C)`.

**Knot write (compact).** A compact is an ordinary `putRecord`. `ConcurrentHeads` still applies: the compact’s `prev` MUST not orphan the current MST value. Last-ts-wins collections share none of this mechanism in version 1.

## Page slugs

`pub.tangled.page` uses `rkey` as the page slug. The rkey charset forbids `/`, so nested pages use `.` as the path separator.

**Encoding (normative):**

- A slug is a sequence of one or more **segments** joined by `.`.
- Each segment MUST match `[A-Za-z0-9_~:!-]+` (the rkey charset minus `.`) and MUST be at least one character.
- The complete rkey MUST match `[A-Za-z0-9._~:!-]{1,256}` and MUST NOT start with `.`, end with `.`, or contain `..`.
- There is no percent-encoding. Characters outside the segment charset MUST NOT appear in the rkey; they belong in the page `title`.
- `.` in the rkey is only a path separator. A single-segment page MUST NOT include `.` (put punctuation in `title`).

**Display (normative):** clients presenting a wiki hierarchy MUST render slug `a.b.c` as `a/b/c`. The stored rkey remains `a.b.c`. Example: rkey `rfc.draft` is the page `rfc/draft`; rkey `Home` is the page `Home`.

## Deletes

A delete is a record with the same `collection` and `rkey`, `body` `null`, `prev` listing the CIDs being deleted, and a new `sig`. Public deletes are tombstones in the MST. Sealed deletes MUST be indistinguishable from other sealed records at the knot (same size class); the body ciphertext decrypts to `null`. Visibility class does not change this shape.

For Last-ts-wins collections, the tombstone is live when it wins among causal heads. For CRDT collections, the live value is `null` only when **every** causal head in `S` is a tombstone. A tombstone concurrent with an edit does not drop that edit: the page stays, and apply uses every non-tombstone record in `S`. A writer that means to delete the page MUST list every causal head it knows in `prev`.

Knots MAY garbage-collect blob bytes after a delete. They MUST keep the tombstone so sync converges. They MUST NOT garbage-collect CRDT op records that remain in some replica’s `prev`-closure, and they MUST NOT drop `I(C)` blocks of a page compact ([Compaction](#compaction)).
