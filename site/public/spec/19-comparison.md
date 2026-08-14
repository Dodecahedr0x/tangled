---
title: Comparison
slug: comparison
order: 19
---

# Comparison

This chapter is evidence-based. Claims about other protocols cite their own specs and widely documented behavior. Tangled does not win “every dimension including network effects”; it has no incumbent graph. That is stated here so marketing cannot creep in.

## Identity

| Dimension | Nostr | AT Protocol | ActivityPub | Farcaster | Tangled |
| --- | --- | --- | --- | --- | --- |
| Primitive | secp256k1 key | DID (`did:plc` default) | `https` actor URI | On-chain FID | Solana Ed25519 key, `did:sol` generative |
| Rotation | Not in NIP-01 | Rotation keys + PLC log | Change servers, new URI | Custody key transfer | Rotation keys in `sys/id`; optional on-chain `did:sol` |
| Recovery | None | 72h window via PLC | Admin | Recovery address | Recovery keys in `sys/id`, 72h delay |
| Handle | NIP-05 | DNS | `@user@host` | Fname / ENS | DNS and SNS, bidirectional |
| Who seizes the account | Nobody (and nobody restores) | Not the DID; PDS can takedown | Home-server admin | Recovery address; Fname registry | Nobody for the DID; DNS/SNS for the handle |
| Cost to exist | Free | Free | Free (operator pays) | Gas + rent | Free; Solana rent only if persisting `did:sol` on-chain |

Nostr NIP-01: the public key is the account; lost `nsec` is permanent. AT Protocol `did:plc`: a global directory (Bluesky-operated at time of writing) is the default method. ActivityPub: identity is the actor URI on a domain. Farcaster SPEC: FID on OP Mainnet, recurring storage rent.

## Data and sync

| Dimension | Nostr | AT Protocol | ActivityPub | Farcaster | Tangled |
| --- | --- | --- | --- | --- | --- |
| Store | Relays, events | PDS repo (MST) | Server collections | Hubs / Snapchain CRDT | Knot repo (MST) |
| Sync | Subscribe to events | Firehose + pull | Inbox POST | Gossip CRDT | Pull MST; optional loom firehose of **public** heads |
| Self-host cost | Cheap relay | Cheap PDS, expensive relay/AppView | Full server | Hub | Cheap knot; loom optional |
| Portability | Rebroadcast events | CAR + DID | Followers, not posts (Mastodon) | FID transfer | CAR + DID |

Mastodon’s own move docs: posts do not move. AT Protocol repository spec: CAR export. ActivityPub delivery: POST to inboxes (N²). AT Protocol federation architecture: relays aggregate PDS firehoses.

## Access control and confidentiality

| Dimension | Nostr | AT Protocol | ActivityPub | Farcaster | Tangled |
| --- | --- | --- | --- | --- | --- |
| Authorization | Relay policy; otherwise public | Public repo; permissioned-data proposal is a **second** protocol, “access control, not confidentiality” | Addressing (`to`/`cc`/`bto`), often ignored | Public hubs | Capabilities on the **same** repo |
| Confidentiality | NIP-44 DMs | Out of scope in permissioned-data proposal | Not E2EE | Not E2EE | `vis: sealed` + HPKE and MLS |
| Groups | Relays / NIP-29 | Spaces (proposal) | Groups as actors | Channels | Circles; size migration invisible |

AT Protocol proposal 0016 (permissioned data): quote, “this protocol provides access control, not confidentiality.” Tangled refuses that split.

## Discovery and moderation

| Dimension | Nostr | AT Protocol | ActivityPub | Farcaster | Tangled |
| --- | --- | --- | --- | --- | --- |
| Global index | Many relays, hope | Relay + AppView, de facto required to see the network | Instance timelines | Hubs | Loom optional |
| Moderation | Client mute, relay ban | Labels + AppView | Instance admin | App-edge | Labels; knot hosting policy |

## Interoperability

Nostr, AT Protocol, and ActivityPub do not specify round-tripping each other. Tangled specifies bindings, envelopes with original bytes, and translation tables. ActivityPub imports remain attested because the AP spec has no author signature.

## Where Tangled is not better

- **Network effects.** Zero users at spec time.
- **Implementation maturity.** This document is a spec, not a running network.
- **Human onboarding without keys.** ActivityPub email-and-password on a server is easier. Tangled can custodially sign (`custodialSign`), at the cost of knot trust.
- **Private global search.** Impossible if sealed; AT Protocol permissioned-data also refuses this, honestly.
- **Handle independence from DNS/SNS.** Zooko’s triangle. Same class of problem as everyone else.

## Summary sentence

Tangled takes Nostr’s key-as-account, AT Protocol’s signed Merkle repo and labels, ActivityPub’s self-hosting, and none of their fused mistakes: un-rotatable keys, a required global directory or firehose, server-custodial identity, or access control without confidentiality.
