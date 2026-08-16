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
| Replay an invocation | `nonce` + `ts` on Invocation; knots SHOULD track nonces; `max_uses` counted per leaf capability CID and persisted across restart | Distributed knots need shared nonce and use-count state |
| Clock skew rejects a valid grant | `CAPABILITY_CLOCK_SKEW_SECONDS` (60) on `nbf` / `exp` / `valid_until` | Attackers get a 60-second extra window after `exp` |

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
| Malicious Welcome | Verify credential = member Solana key; ignore Welcomes not in the circle log signed by an authorized committer; Welcome is sealed to KeyPackage `init_key`, which MUST differ from the leaf/`agreement` key; adders MUST ignore expired `sys.mls.keypackage` records | Clients that HPKE-open Welcome with `agreement` fail; that is required; a stale published KeyPackage can be used until `exp` |
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
| `node.stats` as an oracle for private activity | Counts and `hosted` are live `vis: public` tips only; learned peers, cap/sealed records, capability uses, and sessions MUST NOT appear | Traffic analysis of who pulls stats; public commit volume is visible |
| Inflated or omitted public counts | Overlay rule: treat as this observer’s view; crawl another advertised peer | Users who treat one knot’s stats as the whole network |

## Availability

| Attack | Control | Residual |
| --- | --- | --- |
| Knot downtime | Replicas, CAR export, client cache | If nobody replicated and the operator has no CAR, data is gone |
| Blob withholding | CID mismatch detected; replica | Same as knot downtime for those bytes |

## CRDT operations

A Conflict-free Replicated Data Type (CRDT) op is a signed record. There is no unsigned op gossip.

| Attack | Control | Residual |
| --- | --- | --- |
| Concurrent wiki writes silently drop a head | Page CRDT applies every signed op in the apply set (and compact snapshots); `ConcurrentHeads` on `putRecord` if the new record would orphan the Merkle Search Tree (MST) tip | Two disconnected replicas of the same Decentralized Identifier (DID) can each accept a different tip until a client pulls both and writes a merge `prev` |
| Malicious collaborator inflates the op log (tiny inserts, spam tombstones) | `PAGE_TEXT_MAX_SCALARS` (1 048 576) and `PAGE_RGA_MAX_ATOMS` (4 194 304); refuse to materialize over-bound pages; knot `RateLimited` | Live Replicated Growable Array (RGA) still holds tombstoned atoms; compaction shrinks the record apply set, not the atom cap |
| Compact omits a concurrent op | Snapshot MUST equal Apply of the included set; extra ops concurrent with the frontier are merged on top; reader detects stale vs invalid as in [Data model](./06-data-model.md#compaction) | A replica that never sees the extra op materializes without it until it pulls the replica that has it |
| Compact spam (tiny or empty folds) | `PAGE_COMPACT_MIN_OPS` (32) and `PAGE_COMPACT_MIN_INTERVAL_MS` (300 000); knot `RateLimited`; `repo/write` required | 32 new ops plus interval still let a writer publish large snapshots |
| Knot drops `I(C)` blocks | Authoring knot MUST retain `I(C)`; `getRepo` / `getBlocks` MUST serve them; clients MUST reject a compact whose `I(C)` cannot be fetched and replayed | A replica that has not merged still needs another pull for concurrent heads |
| Forged op | Each op is a signed Tangled record; unsigned gossip is not a transport; a compact snapshot MUST equal Apply(`I(C)`) | Stolen `repo/write` on the shared-repo DID can still write a compact; verifiers detect a lie when `I(C)` is available |
| Client displays a writer buffer instead of apply set `S` | Page bodies are incremental ops or a compact snapshot; clients MUST apply the CRDT (RGA text, Last-Writer-Wins (LWW) title/blob, compact union) | Non-conforming clients diverge |
| Tombstone used to hide a concurrent edit | CRDT delete is live only when every causal head is a tombstone; compact MUST NOT be emitted when every head is a tombstone | A writer who omits a head from `prev` does not delete that head’s ops |

## Out of scope

- Traffic analysis of who pulls which DID (use Tor or equivalent if needed)
- Quantum break of Ed25519/X25519 (future version)
- Legal compulsion of a knot operator (sealed records still hide bodies; metadata may not)
