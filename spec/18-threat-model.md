---
title: Threat model
slug: threat-model
order: 18
---

# Threat model

Assumptions: Ed25519, BLAKE3, HPKE, and MLS are as specified in their RFCs. Attackers include malicious knots, stolen wallets, leaked capabilities, hostile looms, hostile gateways, DNS/SNS operators, and other members of a circle.

## Identity

| Attack | Control | Residual |
| --- | --- | --- |
| Steal identity key | Rotation keys; hardware wallet; device keys for daily use | If rotation keys are stolen too, the attacker is the identity |
| Steal device key | Capability TTL and revocation | Offline verifiers may accept until TTL |
| Hostile knot serves stale `sys/id` | Cached CID, DNS/SNS `head=` hint, replicas | First-time clients with only a knot URL can be lied to until they compare a second source |
| PLC-style directory seizure | No PLC | None of that class |
| DNS or SNS handle seizure | DID remains; handle moves | Users who only know the handle can be phished |

## Signatures

| Attack | Control | Residual |
| --- | --- | --- |
| Confuse a record sig with a Solana transaction | Mandatory `\xffsolana offchain` envelope | Implementations that skip the envelope are non-conforming and unsafe |
| Replay an invocation | `nonce` + `ts` on Invocation; knots SHOULD track nonces | Distributed knots need shared nonce state or short clock skew |

## Visibility

| Attack | Control | Residual |
| --- | --- | --- |
| Knot reads `vis: cap` | Expected; do not put secrets in `cap` | Operator compromise |
| Knot reads `sealed` | Epoch secrets and HPKE recipient keys MUST NOT live on the knot | Client malware; knot that is also the client |
| Listing sealed collections leaks rkeys | Encrypted rkeys | Existence of sealed records and, if `hideMembers` is false, wrap rkeys leak membership to the knot |
| Dual-stack window: HPKE wrap of the new epoch secret lives too long | `DUAL_STACK_MAX_SKEW_*`; stop wraps after | An offline member’s wrap can be fetched by anyone with a leftover `repo/read` cap during the window |
| Agreement rotation: leftover wrap to the old X25519 key | `AGREEMENT_ROTATION_MAX_SKEW_SECONDS`; wrap to both keys only during the window; rotating client discards the old agreement private key after | Offline device of the rotator that misses the window needs a republish from another of their devices |

## Circles

| Attack | Control | Residual |
| --- | --- | --- |
| Removed member reads new messages | New epoch; they are not wrapped; MLS Remove | They keep old epoch secrets |
| Compromised member | They can read current epoch; MLS updates after remove | Past messages they already decrypted |
| MLS epoch skip / fork | Linearize on circle commit log; verify MLS epoch is monotonic | Split-brain if two owners ignore linearization — policy `invite: owner` avoids dual owners |
| Forced-upgrade DoS (spam adds to trigger MLS) | `max_adds_per_hour`; not a user dialog | Determined owner can still grow |
| Stolen or leftover membership grant | Revoke on remove; epoch advance | Fetch of ciphertext until revocation is visible; decrypt still needs epoch secrets |
| Malicious Welcome | Verify credential = member Solana key; ignore Welcomes not in the circle log signed by an authorized committer | None if clients skip verification |
| Identity/agreement rotation without re-wrap | Rotating member’s client MUST re-wrap and MLS-Update; not a user prompt | Offline rotator device past max-skew cannot decrypt until republish |

## Discovery and gateways

| Attack | Control | Residual |
| --- | --- | --- |
| Loom omits you | Choose another loom; pull direct | Reach among people who only use that loom |
| Loom or knot inflates a comment count | Counts are untrusted overlays (`overlay: true`); clients MUST label the source and MAY verify hits | Users who treat a count as canonical |
| Gateway forges ActivityPub | `attested: true` in UI; no author sig exists to check | Users who ignore the badge |
| Gateway drops `bytes_cid` and re-encodes | Spec forbids; clients SHOULD prefer envelopes with `bytes_cid` | Non-conforming gateways |
| Two gateways assign different `foreign_id`s | Canonical `foreign_id` per protocol; rkey MUST be BLAKE3 of `protocol`, a 0x00 byte, and `foreign_id`; duplicates detected by `bytes_cid` or id-inside-bytes | Envelopes written before canonicalization remain until merged |
| Selective translation | Another gateway; original network remains | Split view |

## Availability

| Attack | Control | Residual |
| --- | --- | --- |
| Knot downtime | Replicas, CAR export, client cache | If nobody replicated and the operator has no CAR, data is gone |
| Blob withholding | CID mismatch detected; replica | Same as knot downtime for those bytes |

## Out of scope

- Traffic analysis of who pulls which DID (use Tor or equivalent if needed)
- Quantum break of Ed25519/X25519 (future version)
- Legal compulsion of a knot operator (sealed records still hide bodies; metadata may not)
