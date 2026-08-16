---
title: Abstract
slug: abstract
order: 0
---

# Abstract

Tangled is a content-network protocol. An identity is a Solana Ed25519 public key. Content lives in a signed Merkle repository on a host the identity controls (a **knot**). Authorization is object-capability. Confidentiality is encryption. Discovery and moderation are unprivileged overlays. Other networks bind in with provenance, they do not define the data model.

This document set is the implementable specification of Tangled version 1. Normative language follows [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174), as restated in [Conformance](./02-conformance.md). Every acronym used in these documents is defined in [Terminology](./01-terminology.md).

## What Tangled is

A **substrate**. Records, blobs, graphs, and media are first-class. Microblogs, long-form publishing, private groups, and datasets are applications on top of one data plane. The protocol does not privilege a social-network shape.

A **self-certifying store**. A knot is a convenient place to keep bytes. It is not an authority over identity, history, or meaning. Any replica of a repository can be verified from the signatures and the Merkle root.

A **self-hosting unit**. One process and a local database is enough to exist on the network. Global indexes (looms) and foreign-protocol facades (gateways) are optional.

## What Tangled is not

- Not a blockchain. Records are not sequenced by a global ledger. Solana keys and signatures are the identity and authenticity primitive; optional `did:sol` on-chain persistence is a transparency log, not a requirement to exist.
- Not a message bus. Followers pull. Knots do not fan out inbox POSTs.
- Not a permissioned-data fork. Public, capability-gated, and sealed records share one repository and one sync protocol. Visibility is a field on the record.
- Not a live network in this repository. This repository specifies the protocol and presents it. It does not run knots.

## Claims, and the tradeoffs they refuse

| Claim | What it refuses |
| --- | --- |
| Identity is a Solana key, rotatable in-repo | Nostr’s un-rotatable key; AT Protocol’s PLC directory as a gate |
| One data plane | AT Protocol’s split into a public Merkle Search Tree (MST) protocol and a separate permissioned protocol that provides “access control, not confidentiality” |
| Capabilities gate fetch; encryption gates read | ActivityPub addressing that implementations ignore; AT Protocol permissioned data that the host can read |
| Pull sync of a Merkle tree | ActivityPub N² delivery; Nostr full-event flood as the only sync |
| Looms are unprivileged | AT Protocol relays and AppViews as required infrastructure |
| Gateways with provenance | One-off bridges that cannot round-trip |

Honest losses, stated here so the rest of the spec does not have to apologize for them: Tangled has no incumbent graph. Private records cannot be globally indexed. Capability revocation needs a time-to-live (TTL) or an online check. Human handles lean on the Domain Name System (DNS) or Solana Name Service (SNS). ActivityPub (AP) imports are gateway-attested because AP objects are not author-signed.

## Document map

| Document | Contents |
| --- | --- |
| [Terminology](./01-terminology.md) | Acronyms and glossary |
| [Conformance](./02-conformance.md) | RFC 2119, profiles, node capabilities |
| [Architecture](./03-architecture.md) | Actors, data plane, trust boundaries |
| [Identifiers](./04-identifiers.md) | `did:sol`, handles, rotation, device keys |
| [Cryptography](./05-cryptography.md) | Solana signatures, BLAKE3, HPKE, MLS |
| [Data model](./06-data-model.md) | Records, blobs, schemas |
| [Repositories](./07-repositories.md) | Merkle Search Tree, commits, proofs |
| [Visibility](./08-visibility.md) | `public`, `cap`, `sealed` |
| [Capabilities](./09-capabilities.md) | UCAN-profile certificates |
| [Circles](./10-circles.md) | Groups, epoch secrets, seamless size migration |
| [Synchronization](./11-sync.md) | Pull, CAR, live subscribe |
| [HTTP API](./12-http-api.md) | Methods, content negotiation, errors |
| [Resolution](./13-resolution.md) | DNS, SNS, knot location |
| [Looms](./14-looms.md) | Optional public indexes |
| [Labels](./15-labels.md) | Moderation as signed records |
| [Interoperability](./16-interoperability.md) | Bindings, envelopes, gateways |
| [Migration](./17-migration.md) | Export, import, multi-device |
| [Threat model](./18-threat-model.md) | Attackers, controls, residual risk |
| [Comparison](./19-comparison.md) | ActivityPub, Nostr, AT Protocol, Farcaster |
| [Use cases](./20-use-cases.md) | Worked examples (informative) |
| [Changelog](./21-changelog.md) | Normative v1 additions after the first complete draft |

Machine-readable companions: Concise Data Definition Language (CDDL) under `cddl/`, core schemas under `schemas/`, OpenAPI under `http/`, translation tables under `interop/`, test vectors under `vectors/`.
