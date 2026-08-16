---
title: Visibility and access control
slug: visibility
order: 8
---

# Visibility and access control

Visibility is a field on the record. It is not a second protocol. Fetch (authorization) and read (confidentiality) are different.

## Classes

### public

- Body is plaintext.
- Anyone MAY fetch. Knots advertising `knot.public` MUST serve public records without a capability.
- Anyone MAY rebroadcast, index, and prove inclusion.
- Looms MAY ingest.

### cap

- Body is plaintext on the knot.
- The knot MUST NOT serve the record, its MST path, or its blobs unless the request presents a valid capability for `repo/read` on that DID, collection prefix, or record CID.
- The knot operator can read plaintext. This class is authorization, not confidentiality. Use it for data the host is allowed to see (server-side search the user opted into, collaborative docs on a trusted knot).
- Looms MUST NOT ingest.

### sealed

- Body is `Ciphertext` (or a header pointing at a ciphertext blob).
- Fetch rules are the same as `cap`: a capability is REQUIRED.
- The knot MUST treat the body as opaque. It MUST NOT log decoded bodies. It cannot decode them without epoch secrets or HPKE recipient keys, which MUST NOT live on the knot except in a client running on the same machine.
- Looms MUST NOT ingest.
- Encrypted rkeys: see [Repositories](./07-repositories.md#sealed-collections-in-the-mst).

## Capability presentation

Requests for `cap` and `sealed` MUST include:

```
Authorization: Cap <base64url(DAG-CBOR(Invocation))>
```

The payload MUST be DAG-CBOR. Knots MUST reject an `Authorization` value whose decoded bytes look like JSON (first byte `{` or `[`). JSON-in-header MUST NOT be accepted. The invocation is specified in [Capabilities](./09-capabilities.md). Knots that do not advertise `knot.cap` MUST return `ProfileRequired`.

## What a knot is allowed to know

| Class | Knot sees plaintext body | Knot sees collection and rkey | Knot sees recipients |
| --- | --- | --- | --- |
| public | Yes | Yes | N/A |
| cap | Yes | Yes | N/A |
| sealed | No | Encrypted rkey only | HPKE `enc` bytes, not identities, unless wraps are stored as records the knot can read — wraps are themselves `vis: sealed` |

HPKE wraps of epoch secrets (`sys.circle.wrap`) are `vis: sealed` with `alg = "hpke-record"` to each member. A knot that stores them sees ciphertext. Member identity is in the rkey `{epoch}-{member-base58}` — that leaks membership to the knot. Circle definitions MAY set `hideMembers: true`, in which case wrap rkeys MUST be random TIDs and the member mapping lives inside the ciphertext. Default is `hideMembers: false` because it makes debugging and capability grants tractable. Large-group MLS Welcomes already leak membership to anyone who can fetch the circle log; `hideMembers` is best-effort against the knot, not against members.

## Mixing classes in one repo

A repository MAY contain all three classes. Sync of public collections uses ordinary getRepo. Sync of `cap`/`sealed` collections requires a capability and returns only those collections the capability authorizes. A `repo/read` capability with resource `did:sol:…/pub.tangled.chat` MUST NOT leak sibling collections. A `circle/read` capability MUST return only records whose envelope `circle` matches the resource (plus the circle log on the owner’s repo, as specified in [Circles](./10-circles.md#membership-grant)).

## Blobs

A blob has no `vis` field. Authorization follows records that reference it:

- If at least one **non-deleted** `vis: public` record in that DID’s repository references the blob CID, `tangled.sync.getBlob` MUST serve it without a capability.
- Otherwise the knot MUST require a valid `blob/get` on `tangled://<did>/blob/<cid>`, or a `repo/read` that covers a referencing record, or a `circle/read` that covers a circle-scoped referencing record.
- The same CID referenced from both a public record and a sealed record is public. Clients that need confidentiality MUST AEAD the bytes and store ciphertext.

## Deletes and takedown

Three distinct events. Implementations MUST NOT conflate them.

**Author delete.** A tombstone: same `collection` and `rkey`, `body` `null`, `prev` listing the CIDs being deleted. Public tombstones remain in the MST so sync converges. Sealed deletes MUST be indistinguishable from other sealed records at the knot (same size class); the body ciphertext decrypts to `null`.

**Knot hosting refusal.** A knot MAY stop serving any record (hosting policy). It MUST NOT rewrite a record’s bytes. It MAY omit the record from `getRepo` and MUST answer `getRecord` with `NotFound` or `Unavailable`. This is not a tombstone. Replicas that still hold the commit MAY serve the original bytes after signature verification. Clients MUST NOT treat origin `NotFound` as an author delete when a replica presents a valid inclusion proof for the same head, or when the origin head still includes the key.

**Label.** A signed opinion. It does not delete or authorize refusal. See [Labels](./15-labels.md).

Takedown of public data is hosting refusal, optionally plus a label. Replicas and CAR exports remain valid. This is verifiable exit, not a global delete. Knots MUST still export a CAR of what they do host.
