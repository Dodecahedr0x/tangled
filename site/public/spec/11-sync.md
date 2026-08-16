---
title: Synchronization
slug: sync
order: 11
---

# Synchronization

Followers pull. Authors do not push into N inboxes. Relays are not the source of truth. The source of truth is a signed commit CID (the **head**) and the Merkle Search Tree (MST) behind it.

## Head

`GET /xrpc/tangled.sync.getHead?did=`

Returns `{ did, commit, rev, knot, data }`. `commit` is the head **Commit** CID (the signed Commit object, not the raw MST root). `data` is `Commit.data` (the MST root CID). No capability for public repos. `repo/read` required if the knot hides the DID (unusual; default is to advertise heads).

Clients MUST cache `(did, commit, rev)` and detect rollback: a head with `rev` less than a previously verified `rev` for the same DID is an attack or a recovery race. Clients MUST refuse to adopt that head except as specified for recovery in [Identifiers](./04-identifiers.md#rotation). Recovery races are specified there.

## Get record

`GET /xrpc/tangled.sync.getRecord?did=&collection=&rkey=`

Returns the record block, the MST inclusion proof, and the commit. For `vis: cap` and `vis: sealed`, a capability is REQUIRED. For sealed collections the caller MAY pass `rkey` as the encrypted key they computed.

## Get blocks

`POST /xrpc/tangled.sync.getBlocks` with body `{ cids: [cid] }` (max 256). Returns a CAR of those blocks. Missing CIDs are listed in `missing`.

Version 1 uses a **single MST**. Public collections are served without a capability. When walking a proof for a public record, interior nodes MAY mention sibling keys, including encrypted rkeys of sealed records. Sibling keys leak the existence of other records; that is acceptable. Sibling values are CIDs; fetching a non-public record CID still requires a capability. Proof nodes themselves are not confidential. Knots MUST serve MST nodes for public proofs without a capability, even if siblings are sealed keys.

`getBlocks` of a non-public **record** CID requires a capability that covers that record (`repo/read`, or `circle/read` if the record is circle-scoped). `getBlocks` of a **blob** CID MUST follow the same authorization as `getBlob`.

## Get blob

`GET /xrpc/tangled.sync.getBlob?did=&cid=`

Returns raw bytes (`Content-Type: application/octet-stream`) whose BLAKE3-256 matches the CID (codec `raw` 0x55). Authorization is specified in [Visibility](./08-visibility.md#blobs): serve without a capability only if a non-deleted public record in that DID’s repository references the CID; otherwise require `blob/get`, `repo/read` covering a referencing record, or `circle/read` covering a circle-scoped referencing record.

Knots advertising `knot.public` MUST implement this method for publicly referenced blobs. Authorization vectors: `spec/vectors/getblob-auth.json`.

## Get repo

`GET /xrpc/tangled.sync.getRepo?did=&since=`

Returns a CAR. Without `since`, the full public repo (commits, public MST snapshots, public records). With `since` (a commit CID), the commit chain after `since` through head, the `since` commit block, and the MST nodes plus records needed to reconstruct those commits. Blobs are not included unless `blobs=true`. For `pub.tangled.page`, the CAR MUST include `I(C)` blocks of each compact ([Data model](./06-data-model.md#compaction)). It MUST still include the compact record.

Authorized variant: `POST /xrpc/tangled.sync.getRepoAuth` with invocation, returns authorized collections. If the invocation command is `circle/read`, the CAR MUST include only records the capability authorizes (circle-scoped records on this DID, plus the circle log when this DID is the owner) and MUST NOT include sibling collections. `I(C)` retention follows the same rule as `getRepo`. Import of that CAR MUST verify the `prev` chain.

## Notify (optional)

`POST /xrpc/tangled.sync.notify` body `{ did, commit }` — a hint that a head moved. Knots MAY ignore. Used by replicas and looms. MUST NOT be required for correctness.

## Live

`GET /xrpc/tangled.sync.subscribe?did=&collection=&since=` Upgrade: WebSocket.

Messages: `{ t: "commit", commit: cid, ops: [{action, collection, rkey, cid}] }`.

`since` is OPTIONAL and is a commit CID. When `since` is present and the knot still holds later commit events for that DID (and `collection` filter), it MUST send those events first, in store order, then continue live. When `since` is omitted, the knot MAY send the current head as a hello with empty `ops`. When the knot does not retain `since`, it MUST send that hello and MUST NOT invent a gap-free history. There is no byte-offset cursor. An online knot MUST persist the commit event log next to the filesystem snapshot. Restart MUST still replay `since` for retained events.

Public collections: no cap. Others: cap on subscribe. Heartbeat every 30 seconds. The server MAY drop slow clients.

## Query backlinks

`GET /xrpc/tangled.sync.queryBacklinks?cid=`

Optional `collection=` (restrict hits) and `limit=` (default 50, max 500).

Returns public records **this knot already stores** whose envelope `ref` equals `cid`, or whose application body `reply` equals `cid` when envelope `ref` is absent. This is a **local backlink index**, not a network search.

```
{
  "hits": [{ "did", "collection", "rkey", "cid", "commit" }],
  "count": 3,
  "overlay": true,
  "complete": false
}
```

Knots advertising `knot.public` SHOULD maintain the index from public records they ingest (hosted repositories, follows, replicas). They MUST NOT crawl other knots in order to answer this method. They MUST NOT include `vis: cap` or `vis: sealed` records. Tombstones MUST NOT appear in `hits`. `count` is `len(hits)` after those filters (or the untruncated count if `limit` clipped `hits`). `overlay` MUST be `true`. `complete` MUST be `false` unless the knot can prove it stores every public annotator (v1 knots cannot; they MUST send `false`).

A knot that does not implement the index MUST return `MethodNotImplemented`. An empty `hits` array and `MethodNotImplemented` MUST NOT be treated as proof that the network has zero comments or annotations. They mean this observer has no public overlay (or no index). Clients MUST union overlays as specified in [Looms](./14-looms.md#annotation-discovery).

Discovery of annotations without knowing every annotator DID is specified in [Looms](./14-looms.md#annotation-discovery).

## Peer discovery

Knots do not run a DHT. A knot MAY advertise other public knot base URLs in `tangled.node.describe` → `peers` ([HTTP API](./12-http-api.md#tangled-node-describe)). When knot A first successfully describes knot B, A MAY store B’s `peers` and try those URLs for further `describe`, `getHead`, `identity.resolve`, or `node.stats`. Only the operator-configured public set is advertised. Learned URLs stay local so a peer cannot trick a knot into advertising an attacker URL.

`tangled.node.stats` is the public aggregate of this knot’s live public tips plus that same advertised peer set ([HTTP API](./12-http-api.md#tangled-node-stats)). Clients MAY crawl `peers` to assemble a network view. The crawl is incomplete by design.

## Follow

A follow is a public record `tangled.graph.follow`. The follower’s client:

1. Resolves the target DID to a knot URL.
2. Pulls `getHead` / `getRepo` on an interval or subscribe.
3. Stores the knot URL in local follow state.

The author’s knot does no extra work per follower. That is the efficiency claim against ActivityPub.

## Replication

A replica holds a copy of a repo. Public repos: anyone MAY replicate. `cap`/`sealed`: `knot/replicate` capability. Replicas advertise themselves in their own `sys/id` or in a loom. Clients MAY fail over to a replica if the author’s knot is down, after verifying the commit signature.

## Diff cost

MST diff is O(changed keys × tree height). Tree height is ~log16(n). This is the efficiency claim against Nostr’s “replay all events from all relays.”
