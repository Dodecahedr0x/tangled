---
title: Migration and multi-device
slug: migration
order: 17
---

# Migration and multi-device

Account portability is a CAR plus a DID. The knot is a location.

## Export

`GET /xrpc/tangled.sync.getRepo?did=&blobs=true` with a `repo/read` capability covering all collections (identity session suffices on the home knot). The CAR MUST include commits, MST nodes, records of every visibility class the capability allows, and referenced blobs.

The operator SHOULD also export device capability private keys they still need, out of band. The CAR does not contain private keys.

## Import

On the destination knot:

1. `POST /xrpc/tangled.repo.import` with the CAR and a SIWS session for the DID **or** `repo/write` (method defined in OpenAPI). The knot verifies commit signatures.
2. Write a new `sys/id` with the new knot in `endpoints` (first).
3. Sign a new commit (rotation commit if `pubkey` also changes; otherwise a location-only commit signed by the identity key).
4. Update DNS TXT / well-known / SNS to the same DID (handle unchanged) and optional `head=` hint.

Followers pull the new endpoint from `sys/id`. There is no follower-move protocol. Old knots MAY 301 `getHead` to the new URL.

## Multi-device

Each device gets a device key and a `repo/write` capability with a TTL. Revoke by `sys.revocation`. After that revocation, knots MUST reject subsequent `putRecord` (and other writes) that present the revoked capability (`CapabilityInvalid`). The identity key stays on a hardware wallet or an offline machine.

Web clients: either the browser holds a device key in non-extractable WebCrypto, or the knot custodially signs (`custodialSign: true`, disclosed). Custodial signing means that knot can write as the user. It is OPTIONAL and NOT RECOMMENDED for sealed circles.

## Backup

Any replica with `knot/replicate` is a backup. The operator MAY copy the SQLite file; that is an implementation convenience, not a protocol object.

## Circle membership across move

Circle Welcomes and membership grants are addressed to DIDs, not knots. Moving the knot does not change membership. The member’s client still decrypts. If `history_share` was false, old epochs remain as they were. Device-key revocation (`sys.revocation` of a `repo/write` capability) does not remove the DID from circles; other devices of the same DID continue.

Moving a knot MUST NOT rotate the identity or agreement keys. Location is `sys/id.endpoints`.

If the operator **also** rotates the identity key and/or the X25519 `agreement` key, that is [Identifiers](./04-identifiers.md#rotation) plus [Circles](./10-circles.md#agreement-rotation): the rotating member’s client MUST re-wrap circle epoch secrets to the new agreement key (Hybrid Public Key Encryption (HPKE) wraps and, when the distributor is Messaging Layer Security (MLS), an MLS Update/Commit). That MUST NOT be presented as a user-facing cryptographic mode. Old agreement wraps remain valid for `AGREEMENT_ROTATION_MAX_SKEW_SECONDS`. Device-key-only rotation MUST NOT re-wrap.

The old knot MAY retain sealed ciphertext until the operator deletes it. Copies that already left do not come back. That is not fixed by changing `endpoints`.

## What does not migrate automatically

- Loom indexes (they recrawl)
- SIWS sessions on the old knot
- Gateway-attested AP follower counts on foreign servers (those servers follow the actor URI, which a gateway may keep stable)
