---
title: Compare
slug: comparison
description: How Tangled's identity, signed store, and access model compare with neighboring protocols.
order: 2
---

# One store

Tangled puts public posts, capability-gated fetches, and sealed messages in one
signed repository and one synchronization protocol. The
[comparison chapter](../spec/19-comparison.md) gives the protocol-level analysis
and its source references. These are design comparisons, not benchmarks or a
claim that an implementation has passed independent conformance checks.

## Identity and history

An identity is a Solana Ed25519 key, expressed as a generative `did:sol`. Signing
keys can rotate through `sys/id`. A knot stores its signed history; a replica
verifies that history from signatures and Merkle roots. Optional on-chain
persistence is not required to create a generative identity.

The [identifiers chapter](../spec/04-identifiers.md) defines the model. Comparisons
with Nostr keys, AT Protocol DIDs, ActivityPub actor URIs, and Farcaster identities
belong to the [normative document set](../spec/19-comparison.md), so the site does
not maintain a second competing protocol table.

## Fetch and confidentiality

Capabilities govern which bytes a knot serves. Sealed bodies protect plaintext
from a host that does not hold the decryption keys. A provider that holds an
account's keys can read that account's private content. Custodial hosting is an
explicit choice and must be disclosed.

Public, `cap`, and `sealed` records share the same tree. They do not have the same
privacy: `cap` bodies are readable by their host, and public proofs still expose
some structural metadata. See [Visibility](../spec/08-visibility.md).

## Pull and optional discovery

Followers pull signed commits and missing blocks. Optional looms help discover
public content but are not required for two known knots to synchronize. Gateways
preserve provenance; unsigned ActivityPub objects remain gateway-attested.

## Tradeoffs

- The repository does not establish a production network or an incumbent graph.
- Private records cannot be globally indexed.
- Revocation does not recall bytes someone has already fetched.
- Human-readable handles depend on DNS or Solana Name Service.
- Reference code, test fixtures, and seeded demos are not independent certification.

Run the [getting-started commands](./getting-started.md), then check the
[conformance profiles](../spec/02-conformance.md) appropriate to your implementation.
