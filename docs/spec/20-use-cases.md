---
title: Use cases
slug: use-cases
order: 20
group: eval
---

# Use cases

This chapter is **informative**. It walks applications through Tangled version 1 so an implementer can see which records, visibility classes, capabilities, and sync calls are involved. Normative requirements live in the cited chapters. Where a walkthrough needed a protocol rule that was missing, that rule was added there. Acronyms are defined in [Terminology](./01-terminology.md); the first use of each in this chapter is expanded.

Actors are named. Identifiers are abbreviated (`did:sol:A…`). There is no live network.

## 1. Public microblog

**Actors.** Ada self-hosts a `knot.public`. Bo follows Ada. Cyd labels Ada’s post. A loom is optional.

**Records.** Ada writes `pub.tangled.post` (`vis: public`, `text`, optional `media` blob Content Identifiers (CIDs), optional `reply` = parent **record CID**). Bo writes `tangled.graph.follow` (`vis: public`, rkey = Ada’s Decentralized Identifier (DID)-safe, body `subject`, optional `knot` URL). Cyd writes `tangled.label` (`vis: public`) with `uri` `tangled://did:sol:A…/pub.tangled.post/<rkey>` and `val` `spam` or `ok`.

**Vis / caps.** None for public fetch. Device key + `repo/write` if Ada’s identity key is offline ([Identifiers](./04-identifiers.md#device-keys)).

**Sync.** Bo resolves Ada (handle or follow.`knot`), `getHead` / `getRepo` or `subscribe` on Ada’s knot. Replies live in the **replier’s** repo; they are not POSTed into Ada. Thread assembly is: walk `reply` / `ref` CIDs; query knots Bo already talks to (`tangled.sync.queryBacklinks?cid=`); and/or query a loom (`type=ref`). Ada never fans out.

**Failure / threat.** Loom omits Ada: Bo still pulls Ada’s knot. Cyd’s label does not delete the post ([Labels](./15-labels.md)). Ada’s knot can refuse to host; that is not a network delete (use case 8). A client that only pulls Ada sees Ada’s own replies; remote annotations appear after consulting a backlink index. Empty index is not proof that none exist ([Looms](./14-looms.md#annotation-discovery)).

**Coverage.** `reply` is a record CID. Discovery is pull-based: author repo, local backlink index, optional loom. There is no global inbox; that is a protocol limitation with specified client behavior, not an unspecified gap.

## 2. Private circle chat (5 → 20)

**Actors.** Owner O, members M1–M19. M19 is offline during the add that crosses 16. `history_share: false`. Users add members; they never pick a cryptographic distributor.

**Records.** O writes `sys.circle` (`vis: public` or `vis: cap`, never `sealed`) with `policy.history_share = false`. Wraps and Welcomes: `sys.circle.wrap` / `sys.circle.mls`, `vis: sealed`, envelope `circle` set. Each send is `pub.tangled.chat` in the **sender’s** repo, `vis: sealed`, `alg: epoch-aead`, envelope `circle` = `did:sol:O…#<circle-rkey>`. On add, O issues a **membership grant**: `circle/read` on `tangled://circle/did:sol:O…/<rkey>` to the new member ([Circles](./10-circles.md#membership-grant), [Capabilities](./09-capabilities.md)).

**Vis / caps.** Fetch: `circle/read` (cross-repo) or `repo/read` on the sender. Read: `epoch_secret[N]`. The hosting knot sees ciphertext and, unless `hideMembers`, wrap rkeys.

**Sync.** Members list peers from `sys.circle`, resolve each `sys/id`, `getRepoAuth` / `subscribe` with `circle/read`. Annotation pattern: N−1 pulls. Crossing 16 is still “add member”: same circle id, dual-stack epoch, Welcomes in the log. M19 returns, reads via wrap, processes Welcome, continues. Pre-join records do not decrypt (`history_share` false). Shrink 20→5 stays on the large-group distributor ([Circles](./10-circles.md#never-downgrade)).

**Failure / threat.** Removed member keeps old epochs, loses new ones. Forced adds are rate-limited, not a confirm dialog. Offline past dual-stack max-skew: client re-Welcomes on sync; not a user-facing re-invite. Honest loss: membership grants must be revoked on remove or the removed member can still **fetch** ciphertext they can no longer decrypt.

**Coverage.** Size migration was specified. Missing pieces were envelope `circle`, `circle/read`, and how members fetch each other’s annotation-pattern messages.

## 3. Long-form publishing with media and comments

**Actors.** Ada publishes. Bo comments. Media on Ada’s knot.

**Records.** Ada `putBlob` for the article body (markdown) and for images. Ada writes `pub.tangled.media` (`blob`, `mime`, `alt`) and `pub.tangled.article` (`title`, `blob` canonical body, optional `text` preview, `media` CIDs). Bo writes `pub.tangled.annotation` in Bo’s repo with envelope `ref` = article CID.

**Vis / caps.** Public article: anyone `getRecord` / `getBlob`. Capability-gated draft: `vis: cap` until Ada writes a public successor (`prev`).

**Sync.** Readers pull Ada. Comments: pull Bo if known; `tangled.sync.queryBacklinks?cid=<article>` on knots already in use; optional loom `type=ref&q=<article-cid>`. Counts from those queries are overlays (`overlay: true`), never a field Ada can forge as canonical ([Looms](./14-looms.md#comment-counts)). Blobs via `tangled.sync.getBlob` ([Synchronization](./11-sync.md#get-blob)).

**Failure / threat.** Ada tombstones the article; blob becomes non-public once no live public record references it. Bo’s annotation remains in Bo’s repo (dangling `ref` is allowed). A client that never consults an index sees zero remote comments; that is required behavior, not a bug.

**Coverage.** Article schema, `pub.tangled.annotation`, `ref`, `getBlob`, backlink query, overlay counts.

## 4. Shared wiki / dataset (shared-repo)

**Actors.** Owner O creates circle DID `did:sol:W…` (dedicated key) hosted on O’s knot. Collaborators C1, C2.

**Records.** Circle definition as in use case 2. Wiki pages: `pub.tangled.page` in **W’s** Merkle Search Tree (MST), rkey = slug (`Home`, `rfc.draft` displayed as `rfc/draft`). Dataset rows: application collection plus blobs. Concurrent page edits: Conflict-free Replicated Data Type (CRDT) — Replicated Growable Array (RGA) on `text`, Last-Writer-Wins (LWW) registers on `title` and `blob`. Two writers who have not seen each other both keep their inserts; apply order does not pick a whole-page winner. A later compact is a signed snapshot of that apply at a frontier of included op CIDs; a late op concurrent with the compact still applies on top ([Data model](./06-data-model.md#page-crdt), [Compaction](./06-data-model.md#compaction)). Posts and annotations stay Last-ts-wins and do not compact. Slugs MUST NOT contain `/`; `.` is the nested-page separator ([Data model](./06-data-model.md#page-slugs)).

**Vis / caps.** Typical wiki: `vis: cap` (host may search). Confidential wiki: `vis: sealed` + envelope `circle` + epoch Authenticated Encryption with Associated Data (AEAD). O issues to each collaborator: `repo/read` + `repo/write` + `blob/*` attenuated to `tangled://did:sol:W…/pub.tangled.page` (and blob prefix), plus `circle/read` if sealed. Writers sign records with their device key; `author` is `did:sol:W…`; `signedBy` + `proof` required. The knot signs **commits** with a device key of W ([Circles](./10-circles.md#shared-repo-pattern)).

**Sync.** One `subscribe` on W. Not N member pulls.

**Failure / threat.** `vis: cap` means O’s knot operator can read plaintext — that is the point of this class. Stolen collaborator cap: revoke, epoch-advance if sealed. The circle identity key is another secret O must hold; collaborators cannot rotate W. Compact spam is knot-rate-limited. A compact that missed a concurrent edit is stale: clients that have the extra op merge it; they do not drop it.

**Coverage.** Shared-repo pattern, commit signing, `author`/`signedBy`, page CRDT, compaction, nested slugs. Last-ts-wins remains for posts, annotations, and graph keys.

## 5. Cross-network import, export, dual-publish

**Actors.** Gateway G (`gateway.nostr` + `gateway.ap`). Ada bound to a Nostr key (verifiable) and an ActivityPub (AP) actor (attested).

**Records.** `sys.binding` for each foreign id. Import: blob of original bytes; `sys.foreign` with canonical `foreign_id` (Nostr event id, AT strong URI, ActivityPub object `id` Internationalized Resource Identifier (IRI), Farcaster message hash), `bytes_cid`, `translator` = G, `attested` false for Nostr and true for AP; optional mapped `pub.tangled.post`. rkey of `sys.foreign` MUST be hex of BLAKE3 (`protocol || 0x00 || canonical foreign_id`) ([Interoperability](./16-interoperability.md#canonical-foreign_id)).

**Round-trip.** Export MUST emit `bytes_cid` bytes when present. Native Ada post exported to Nostr: G writes `sys.foreign` with `mapped` = Ada’s post CID and `bytes_cid` = the event bytes. Re-export of that CID MUST reuse the envelope and MUST NOT mint a new event id. Re-import MUST NOT create a second post. If two gateways used different `foreign_id`s before canonicalization, clients detect duplicates by `bytes_cid` equality or the protocol id inside the bytes.

**Dual-publish.** Ada’s public post is mapped to an AP `Note` in G’s outbox. If that `Note` returns on Server-to-Server (S2S), the idempotent envelope key collides and G updates the envelope, not the native record.

**Failure / threat.** Clients MUST show AP objects as host-attested. G can lie about AP; G cannot forge a Nostr sig if the client verifies. AP follower counts live on foreign servers; article→Authenticated Transfer Protocol (ATProto) is lossy (`mapped_lossy`).

**Coverage.** Bindings, envelopes, attested vs verifiable, canonical `foreign_id`, idempotent rkeys, native-export `mapped`, duplicate detection.

## 6. Account migration

**Actors.** Ada moves from knot K1 to K2. Same DID, same handle. Laptop device key revoked. Circle member throughout.

**Records.** Export a Content Addressable aRchive (CAR) (`getRepo?blobs=true` with a cap covering all classes). `import` on K2. New `sys/id` with K2 first in `endpoints`. Location commit (identity key). Domain Name System (DNS) / Solana Name Service (SNS) unchanged for the DID; update knot URL / `head=` hint. `sys.revocation` for the laptop capability.

**Vis / caps.** Identity session on K2. Membership grants are to Ada’s DID, not K1; circle membership survives ([Migration](./17-migration.md)). Device revoke does not remove Ada from circles.

**Sync.** Followers re-resolve `sys/id` (max 24h cache). K1 MAY 301 `getHead`. No follower-move protocol.

**Failure / threat.** Sign In With Solana (SIWS) on K1 dies. Looms recrawl. K1 may still hold sealed ciphertext until the operator deletes it; copies that left K1 remain. If nobody replicated and the CAR is lost, data is gone. **Location move does not rotate keys.** If Ada also rotates the identity and agreement keys, her client MUST re-wrap circle epoch secrets to the new X25519 `agreement` key (and MLS-Update if the circle is on MLS) without a user-facing mode ([Circles](./10-circles.md#agreement-rotation), [Migration](./17-migration.md#circle-membership-across-move)). Device-key-only revoke of the laptop does not re-wrap.

**Coverage.** Location vs identity rotation vs device rotation are distinct. Foreign AP followers follow G’s actor URI, not K1.

## 7. Sealed media album (invented)

**Actors.** Ada shares an album with circle C (use case 2), not with the public.

**Records.** For each photo: AEAD the bytes under `epoch_secret[N]`; `putBlob` the ciphertext; `pub.tangled.media` with `vis: sealed`, envelope `circle`, `Ciphertext` header `{alg: epoch-aead, epoch: N, blob: cid}` (`ct` omitted). Album index: `pub.tangled.album` (`title`, `items` = media record CIDs), same vis and `circle`.

**Vis / caps.** `circle/read` + `blob/get` (implied by `circle/read` for referenced blobs). Knot MUST NOT serve these blobs as public. Clients MUST encrypt media bytes; a plaintext blob CID in a sealed body is fetchable by anyone who learns the CID from a leak.

**Sync.** Members pull Ada (and other contributors) as in use case 2; `getBlob` per item.

**Failure / threat.** `history_share: false`: a late joiner does not decrypt old photos. Removed member keeps photos they already fetched. Same CID placed in a **public** record makes the bytes public. Honest loss: encrypted rkeys hide names, not the count of sealed objects on the knot.

**Coverage.** Ciphertext-as-blob existed; `getBlob` authorization and the album schema did not.

## 8. Takedown versus replica (invented)

**Actors.** Ada’s public post. Knot K refuses to keep hosting it (policy). Replica R still has the CAR. Labeler L writes `val: nsfw`.

**Records.** Three different objects, not one:

| Event | What the MST does | What clients see |
| --- | --- | --- |
| Author delete | Tombstone (`body` `null`, `prev` set) | Protocol delete; sync converges |
| Knot hosting refusal | Bytes unchanged if retained; MAY omit from `getRepo` | `NotFound` or `Unavailable`; **not** a tombstone |
| Label | New `tangled.label` | Opinion; compose labelers |

**Sync.** Clients that only talk to K lose availability. Clients that fail over to R, after verifying commit signatures, still see the post unless Ada tombstoned it. K MUST NOT rewrite record bytes.

**Failure / threat.** Users confuse K’s 404 with Ada’s delete. Hostile K withholds; replicas detect by head/rev, not by filling missing records as “deleted.” Honest loss: there is no global takedown.

**Coverage.** Visibility and labels said this in outline; the three-way distinction and replica failover were easy to misread.
