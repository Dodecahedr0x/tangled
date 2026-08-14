---
title: Labels
slug: labels
order: 15
---

# Labels

Moderation is signed opinion, not protocol-level deletion. A **label** is a public record asserting something about a DID or a record CID. Clients subscribe to **labeler** DIDs and apply labels locally. Knots MAY additionally refuse to host (that is hosting policy, not a label).

## Record

Collection `tangled.label`, rkey = TID.

```
Label = {
  v: 1,
  src: did,                 ; labeler, MUST equal author
  uri: text,                ; tangled://did/… or tangled://did/collection/rkey
  val: text,                ; "spam", "nsfw", "scam", "ok", or a namespaced value
  neg: bool,                ; true = retract
  ts: uint
}
```

`val` SHOULD be a short token. Namespaced values (`pub.example.hate`) are allowed. Clients MUST ignore unknown `val` they have no policy for.

## Semantics

- The labeler DID is the authority over its own labels, nothing else.
- A later label with `neg: true` and the same `(src, uri, val)` retracts.
- Labels do not delete data. They advise clients.
- Multiple labelers may disagree. Clients MUST expose composition: which labelers are on.

## Knots

A knot MAY refuse to serve content it will not host. It MUST NOT rewrite. It MAY attach its own labels. It MUST still export a CAR of what it does host.

A knot MUST NOT silently drop public records from `getRepo` **because a labeler disliked them**. Filtering by label is a client (or loom) job. Hosting refusal is a different event: `NotFound` or `Unavailable` for that record, optional omission from `getRepo`, which replicas can fill. Author tombstones are a third event. See [Visibility](./08-visibility.md#deletes-and-takedown).

Clients that fail over to a replica MUST verify the commit signature and MUST NOT treat origin unavailability as a tombstone.

## Comparison

This is AT Protocol’s label idea, kept, without making an AppView mandatory to see the network. ActivityPub instance-admin god-mode is a knot hosting policy, not a network-wide fact. Nostr client-side mute lists are a subset of labels.
