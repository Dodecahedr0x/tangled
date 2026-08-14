---
title: Repositories
slug: repositories
order: 7
---

# Repositories

A repository is a signed Merkle Search Tree (MST) of records plus a blob store. The MST is the same idea as AT Protocol’s repository: keys are `collection/rkey`, values are record CIDs, interior nodes are content-addressed. Tangled uses BLAKE3 and DAG-CBOR instead of SHA-256 and DAG-CBOR-with-sha256.

Partial sync and inclusion proofs are why MST is used for **all** visibility classes, including sealed collections. AT Protocol’s permissioned-data proposal abandons MST for a set hash because “partial sync is less important.” Tangled keeps one code path.

## Keys

MST key = UTF-8 `collection + "/" + rkey`. Keys MUST be unique. Keys MUST NOT contain `0x00`.

## Node structure

Fanout is 4 bits (16-way), as in AT Protocol. A node is:

```
MstNode = {
  l: cid / null,          ; left subtree for keys before this node’s first entry
  e: [+ MstEntry]
}

MstEntry = {
  p: uint,                ; prefix length shared with previous key
  k: bstr,                ; key suffix
  v: cid,                 ; record CID
  t: cid / null           ; right subtree
}
```

Empty repository: no nodes; the data CID on the commit is the CID of an empty node `{l: null, e: []}`.

Implementations MUST produce the same node CID as `spec/vectors/mst-proof.json` for the listed insertions.

## Commit

```
Commit = {
  v: 1,
  did: did,
  data: cid,              ; MST root
  rev: text,              ; TID, monotonically increasing
  prev: cid / null,
  sig: bstr .size 64,
  ? sigRotation: bstr .size 64
}
```

`rev` MUST be greater (TID order) than `prev`’s commit `rev` when `prev` is not null.

The commit CID is BLAKE3 of the complete DAG-CBOR including signatures. The signing envelope hashes the unsigned commit ([Cryptography](./05-cryptography.md#commit-signatures)).

A knot advertises **head** = latest commit CID for a DID.

## Rotation commits

When `sys/id` changes `pubkey`, the commit MUST include `sigRotation` verifiable with a key listed in the **previous** `sys/id.rotation`. Verifiers MUST:

1. Load previous `sys/id` from `prev` commit’s MST.
2. Verify `sig` with the new `pubkey` (the new identity signs its own first commit).
3. Verify `sigRotation` with a previous rotation key.

Recovery commits (signed by `recovery` instead of `rotation`) MUST set `recovery: true` on the commit. Knots SHOULD serve both the old head and the recovery head for `RECOVERY_DELAY_SECONDS`.

## Proofs

An inclusion proof for key `k` is the list of MST nodes from the root to the leaf that holds `k`, plus the record block. A verifier:

1. Hashes the record, checks it equals `v`.
2. Reconstructs parent nodes, checks they equal the links.
3. Checks the root equals `commit.data`.
4. Verifies `commit.sig`.

An exclusion proof is the node path showing where `k` would be and that it is not.

Knots advertising `knot.public` MUST implement `tangled.sync.getRecord` returning a proof.

## CAR export

A repository export is a Content Addressable aRchive (CAR) v1 whose roots are `[commit CID]` and whose blocks are: the commit, every MST node, every record, and optionally blobs. The header `roots` MUST be the commit CID.

Partial CAR: blocks needed for a `since` diff (commits from `since` exclusive to head inclusive, plus MST nodes and records that changed). A CAR MUST include every record in the `prev`-closure of each MST value ([Data model](./06-data-model.md#merge-rules)), except that for `pub.tangled.page` a knot MAY omit records in the included set of a Garbage Collection (GC) root compact as specified in [Compaction](./06-data-model.md#compaction). The compact record itself MUST remain so the pull still proves its inclusion.

## Sealed collections in the MST

Sealed records occupy MST keys. The key is `collection + "/" + encrypted_rkey` where `encrypted_rkey` is hex of `ChaCha20-Poly1305(key=HKDF(epoch_secret, "tangled.rkey"), nonce=0, plaintext=rkey)` truncated to 32 bytes and hex-encoded. Listing a sealed collection therefore does not leak plaintext rkeys. Members who hold `epoch_secret[N]` can map keys back. Knots MUST store whatever key the writer sent; they MUST NOT require plaintext rkeys for `vis: sealed`.

Public and `cap` collections use plaintext `collection/rkey`.

## Multiple identities

One knot MAY host many DIDs. Each DID has its own MST and commit chain. There is no knot-level Merkle root.
