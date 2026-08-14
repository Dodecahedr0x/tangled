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

`prev` lists CIDs of **same-key** causal parents: records that share this record’s `(author, collection, rkey)`. Empty means a root. Cross-record replies and annotations MUST use envelope `ref` (and MAY repeat the target in the application body as `reply`); they MUST NOT use `prev` for a different rkey. Concurrent writes are resolved by **Last-ts-wins among causal heads** ([Merge rule](#merge-rule-last-ts-wins)).

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
| `pub.tangled.page` | wiki page: `title`, `text` and/or `blob`; rkey is the slug encoded as in [Page slugs](#page-slugs) |
| `pub.tangled.chat` | sealed circle message: `text` or blob; body `circle` MUST equal envelope `circle` |

These exist so a substrate is not a secret microblog. Conforming knots MUST sync them like any collection. Conforming clients need not implement their UI. When `pub.tangled.article.blob` is set, clients MUST treat that blob as the canonical body; `text` is a preview.

## Merge rule (Last-ts-wins)

Version 1 does **not** specify a Conflict-free Replicated Data Type (CRDT). The merge rule for records that share `(author, collection, rkey)` is **Last-ts-wins among causal heads**:

1. Record B **supersedes** record A when they share `(author, collection, rkey)` and A’s CID appears in B’s `prev`, or in the `prev` of a same-key ancestor of B.
2. A **causal head** is a same-key record that is not superseded.
3. The **live** record is the causal head with the greatest `ts`. If `ts` is equal, the lexicographically greatest record CID wins (CID compared as multibase base32 text).
4. A **merge record** is a same-key record whose `prev` lists two or more causal heads. Writing one is OPTIONAL. A client that merges concurrent edits MUST include every concurrent head it considered in `prev`. The body is the author’s merged value. There is no automatic field-wise merge.
5. If no merge record exists, step 3 picks one concurrent head. The loser remains in the Merkle Search Tree (MST) until a delete tombstone or a later merge record `prev`s it.
6. A delete (`body` `null`) is an ordinary record under this rule: its `prev` SHOULD list every causal head being deleted.

Implementations MUST apply this rule for `pub.tangled.page` and for any other same-key edit. Clients MUST NOT invent a CRDT for v1.

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

A delete is a record with the same `collection` and `rkey`, `body` `null`, `prev` listing the CIDs being deleted, and a new `sig`. Public deletes are tombstones in the MST. Sealed deletes MUST be indistinguishable from other sealed records at the knot (same size class); the body ciphertext decrypts to `null`.

Knots MAY garbage-collect blob bytes after a delete. They MUST keep the tombstone so sync converges.
