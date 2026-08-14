---
title: Looms
slug: looms
order: 14
---

# Looms

A loom indexes **public** heads. It is an unprivileged overlay. Two knots can sync without one. A client that never talks to a loom can still follow people it knows.

## What a loom does

- Subscribe to or poll `getHead` on knots it knows.
- Ingest public records (and only public records).
- Serve queries: posts mentioning a DID, recent public commits, records whose envelope `ref` or body `reply` equals a CID, hashtag indexes if applications emit them.
- Serve labels it trusts, or pass through labeler DIDs.

## What a loom MUST NOT do

- Require membership for a knot to exist.
- Ingest `vis: cap` or `vis: sealed` records.
- Rewrite records. Indexes point at CIDs.
- Claim to be authoritative for a DID’s head. The author’s knot (and signed commits) are authoritative.

## Discovery of knots

A loom learns knot URLs from:

- Operator configuration
- `tangled.sync.notify`
- Public follow graphs (walk `tangled.graph.follow`, resolve handles)
- Other looms (gossip of `{did, knot, commit}` tuples)

There is no required global firehose. A loom MAY offer one for public data: a WebSocket of public commits. Clients treat it as a hint stream and MUST verify signatures.

## Client composition

Clients SHOULD let the user pick looms, as they pick labelers. A censored loom is left. That is verifiable exit at the discovery layer.

## Node advertisement

A loom is a knot with `profiles` containing `loom.index`. Methods:

- `GET /xrpc/tangled.loom.query?q=&type=`
- `GET /xrpc/tangled.loom.firehose` WebSocket of public commits

These methods are OPTIONAL. They are specified so looms interoperate instead of inventing search APIs.

`tangled.loom.query` returns `{ hits, count, overlay, complete }` for public records only. Each hit is `{ did, collection, rkey, cid, commit }`. When `type=ref`, `q` is a record CID; hits MUST be public records whose envelope `ref` equals `q`, or whose body `reply` equals `q` when envelope `ref` is absent. Looms MUST NOT invent hits for `cap` or `sealed` records.

`count` is the number of distinct matching record CIDs the loom observes (not clipped by a hit page size). `overlay` MUST be `true`. `complete` MUST be `false` in v1 (a loom is not the whole network). Comment counts are specified below.

## Annotation discovery

There is no global reply inbox and no ActivityPub (AP) inbox fan-out. A reader finds annotations on a record by **pulling indexes of records already stored**, never by contacting every annotator Decentralized Identifier (DID).

Version 1 path, in order:

1. **Author repo.** Pull the target author’s repository. Annotations the author wrote (same DID) are visible. Remote annotations are not, unless they happen to live in that repo.
2. **Local backlink index.** `tangled.sync.queryBacklinks?cid=` on knots the client already talks to (the author’s knot, the client’s home knot, replicas). Each knot answers only from public records it already stores. Specified in [Synchronization](./11-sync.md#query-backlinks).
3. **Loom overlay (OPTIONAL).** `tangled.loom.query?type=ref&q=<cid>` on looms the user chose. Looms remain optional: a client that only pulls the article author’s repo MUST still work; it sees zero remote annotations until it consults an index.

Clients MUST union hits by record CID and MUST verify each hit’s signature and MST inclusion before display. Clients MUST NOT treat an empty result as proof that no annotations exist in the network.

Knots SHOULD index `ref` / `reply` as they ingest public records. They MUST NOT start a new pull of unknown DIDs because of a `queryBacklinks` call (that would restore N²).

## Comment counts

A comment **count** is an untrusted overlay. It MUST NOT be a field on the target record that clients treat as canonical: the target author can write any integer, and no reader can verify it without enumerating annotations.

**Computation.** For a target CID `T` and an observer (client, knot, or loom):

```
count(T) = number of distinct public, non-deleted record CIDs
           this observer stores whose envelope ref = T
           (or body reply = T when ref is absent)
```

Typical filter: collection `pub.tangled.annotation` or posts with `reply`. Observers MAY filter further (block lists, labels). They MUST document the filter if they expose `count`.

**Labeling.** Clients MUST present the figure as an overlay bound to its source, for example `12 (this loom)` or `3 (observed locally)`. They MUST NOT present it as an author-signed or globally complete count. `overlay: true` on the query response is the machine label.

Hostile looms can inflate or omit counts. Clients that care MUST verify hits or pick another loom. That is the same trust model as any loom query.
