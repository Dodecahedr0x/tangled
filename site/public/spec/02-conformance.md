---
title: Conformance
slug: conformance
order: 2
---

# Conformance

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **NOT RECOMMENDED**, **MAY**, and **OPTIONAL** in this document set are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174) when, and only when, they appear in all capitals.

## Version

This is Tangled version **1**. The integer `v` on records, commits, and capabilities is `1`. A future version MUST increment `v` and MUST define compatibility with version 1.

## Profiles

An implementation advertises a profile. Profiles are additive. Advertising a profile means the implementation implements every MUST in that profile.

| Profile | Who | MUST implement |
| --- | --- | --- |
| `knot.public` | Any knot | Identity, public records, MST, pull sync, HTTP API for public methods including `tangled.sync.getBlob` for publicly referenced blobs, handle resolution. SHOULD implement `tangled.sync.queryBacklinks` from locally stored public records |
| `knot.cap` | Knots that serve `vis: cap` | Capability verification, revocation fetch or TTL, authorized sync, authorized `getBlob` |
| `knot.sealed` | Knots that store sealed records | Opaque blob storage, encrypted rkeys, no plaintext logging of bodies, `circle/read` for circle-scoped records they store |
| `knot.group` | Knots that host circles | Both Hybrid Public Key Encryption (HPKE) small-group and Messaging Layer Security (MLS) large-group distributors, seamless migration, membership grants, honor `circle/read` |
| `client.verify` | Any client | Signature verification, MST proofs, DID and handle resolution |
| `client.group` | Clients that join circles | Both distributors, dual-stack catch-up, never-downgrade |
| `gateway.ap` | Optional | ActivityPub C2S/S2S facade, attested imports labeled as such |
| `gateway.nostr` | Optional | Nostr NIP-01 relay mapping |
| `gateway.atproto` | Optional | AT Protocol XRPC subset over the same MST |

A knot that only hosts public repositories MAY omit `knot.cap`, `knot.sealed`, and `knot.group`. It MUST advertise the omission in node capabilities ([HTTP API](./12-http-api.md#tangled-node-describe)).

A `client.group` implementation MUST NOT expose Hybrid Public Key Encryption (HPKE) versus Messaging Layer Security (MLS) as a user-facing mode, setting, or prompt.

## Node capabilities document

`GET /xrpc/tangled.node.describe` returns:

```json
{
  "v": 1,
  "did": "did:sol:…",
  "profiles": ["knot.public", "knot.cap"],
  "protocols": [],
  "maxBlob": 134217728
}
```

`protocols` lists foreign facades this knot speaks (`activitypub`, `nostr`, `atproto`). Empty means none. `maxBlob` is bytes.

## Conformance tests

An implementation claiming a profile MUST pass the vectors in `spec/vectors/` that the profile lists:

| Vector file | Profiles |
| --- | --- |
| `unsigned-record.json` | `client.verify`, `knot.public` |
| `solana-signature.json` | `client.verify`, `knot.public` |
| `mst-proof.json` | `client.verify`, `knot.public` |
| `capability-chain.json` | `knot.cap`, `client.verify` |
| `hpke-sealed.json` | `knot.sealed`, `client.group` |
| `mls-welcome-commit.json` | `knot.group`, `client.group` |
| `circle-grow-dual-stack.json` | `knot.group`, `client.group` |
| `handle-dns.json` | `knot.public`, `client.verify` |
| `binding-nostr.json` | `gateway.nostr` or any client that consumes bindings |
| `circle-read-auth.json` | `knot.group`, `knot.sealed`, `client.verify` |
| `getblob-auth.json` | `knot.public`, `knot.cap`, `knot.sealed` |
| `page-crdt-concurrent.json` | `client.verify`; any client that materializes `pub.tangled.page` |
| `page-crdt-compact.json` | `client.verify`; any client that materializes `pub.tangled.page` |

Passing means: given the inputs, the implementation produces the stated digests, accepts the stated signatures, and rejects the stated negative cases. For `page-crdt-concurrent.json`, it also means both listed apply orders produce the stated `live` value. For `page-crdt-compact.json`, it also means Apply(compact) equals Apply(history), the late op is concurrent with the compact, and Apply(compact, late) equals Apply(history, late) in both listed orders.

## Extensibility

Unknown record types MUST be stored and synced if the record is well-formed (valid CID, valid signature, known `v`). Knots MUST NOT drop unknown `type` values on public collections. Clients MUST ignore unknown fields when the CDDL `any` / extra-map rule allows them.

New HTTP methods MUST live under `tangled.*` or an application namespace. Knots MUST return `MethodNotImplemented` for unknown `tangled.*` methods rather than a generic 404 that hides whether the knot is a Tangled knot.

## Normative references

Implementations MUST follow the profiles of:

- RFC 8949 (CBOR) as constrained by the DAG-CBOR canonicalization in [Cryptography](./05-cryptography.md)
- RFC 9180 (HPKE) with the ciphersuite in that chapter
- RFC 9420 (MLS) with ciphersuite `0x0001` for `knot.group` / `client.group`
- [did:sol method](https://identity-com.github.io/sol-did/did-method-spec.html) generative profile
- [Solana off-chain message format](https://docs.anza.xyz/cli/examples/sign-offchain-message)
- UCAN delegation semantics as profiled in [Capabilities](./09-capabilities.md)
