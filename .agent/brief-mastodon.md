# Brief — Mastodon / ActivityPub gateway

## What we're building

A **`tangled-knot` ActivityPub / Mastodon gateway extension**. Mapping lives in a **separate crate** wrapped into the knot: **one process**, extra AP routes on the knot (not a sidecar). The knot speaks AP S2S (inbox/outbox/actor/WebFinger; **WebFinger + HTTP Message Signatures required**) and stores translations as spec-16 envelopes. **Reed stays a Tangled client**: same `crates/tangled` HTTP path (`getHead` / `getRepo` / `putRecord`), same replica. **Do not build Mastodon REST or OAuth in Reed.** Identity, signing, CID, and XRPC stay Tangled-only. Do not extend `PostApi`. Reed shows a tiny **federated / attested mark** on mapped posts.

**Ingress.** Remote `Create{Note}` (and related activities) become attested `sys.foreign` (`protocol: activitypub`, `foreign_id` = object `id` IRI, rkey = BLAKE3(`protocol||0x00||id`)) plus, when mappable, a native `pub.tangled.post`. Field map: `Note.content` → `post.text`; `inReplyTo` → reply; attachments → media. The gateway DID is `translator`. Knots reject unattested AP. Dual-publish (a native public post also emitted as an outbox `Note`) updates the envelope only on rkey collision. Reed `Sync` already `get_head` / `get_repo` / `apply_record`s any collection; timelines hydrate `pub.tangled.post` only.

**Egress.** Reed compose/follow/like/boost/reply stay Tangled writes. The knot maps them out:

| Tangled | ActivityPub |
| --- | --- |
| `pub.tangled.post` | `Note` (`Create`; reply via `inReplyTo`) |
| `tangled.graph.follow` | `Follow` |
| `pub.reed.like` (app collection) or mapped AP `Like` | `Like` |
| `pub.reed.boost` (app collection) or mapped AP `Announce` | `Announce` |

Likes and boosts are **not** in `spec/interop/translations.yaml`. v1 maps Reed app collections ↔ AP `Like` / `Announce` at the gateway; do not mint a new spec type.

**Addressing.** From Reed, paste `acct:user@instance` (or `user@instance` / `@user@instance`) or an actor URL. The **knot gateway** resolves (WebFinger and/or actor GET). Reed does not speak WebFinger itself.

**Reed surface (unchanged protocol path, minimal chrome).** Home = the follow graph including AP actors the knot resolved and ingested. Discover includes federated public Notes the knot ingested (plus public Tangled posts it already knows). Follow accepts `acct:@`; ingested Notes look like Reed posts (`pub.tangled.post`) with a tiny federated/attested mark. Profiles, reply, like, boost work as today. No new Mastodon tabs or compose chrome. No DMs.

## Constraints

- Gateway on the knot, not in Reed. Do not add Mastodon client API/OAuth to `apps/reed`. Reed follow may accept `acct:@` / actor URL text; resolution is a knot job.
- Protocol via `crates/tangled` only. Reuse `foreign::{canonical_foreign_id,import_rkey,activitypub_import_attested,verify_ap_proof}` and the KnotEngine AP attestation gate. No parallel post format. Ingested Notes render as `pub.tangled.post`, not a second post type. Do not extend `PostApi`.
- Separate mapping crate wrapped into `tangled-knot`: same process, extra AP routes on the knot. Identity, signing, CID, and XRPC stay Tangled-only. `tangled-knot` today is XRPC+WS only.
- S2S requires WebFinger and HTTP Message Signatures (`Digest` on POST; RFC9421).
- `Note` ↔ `pub.tangled.post` (`content`→`text`, `inReplyTo`→reply, attachments→media); follow via `tangled.graph.follow`; AP `Like` → `pub.reed.like`; AP `Announce` → `pub.reed.boost`. Preserve `bytes_cid` for round-trip. Set `mapped_lossy` when the mapping stubs.
- Clients MUST surface `attested` for imported AP objects (spec 16). AP is gateway-attested, never key-equals-person. Reed shows a tiny federated/attested mark on mapped posts.
- No DMs, no APNs/FCM, no fake public network. Do not overwrite Reed `brief.md` / tree or existing `research/*.md`.

## Success criteria

1. Knot advertises `activitypub` on `describe.protocols`. Inbox `Create{Note}` writes attested `sys.foreign` + mapped `pub.tangled.post`; Reed Home shows follows including AP actors; Discover includes those federated public Notes via existing sync.
2. Reed `putRecord` of post / reply / `tangled.graph.follow` / `pub.reed.like` / `pub.reed.boost` causes the knot to emit `Create{Note}` / `Follow` / `Like` / `Announce` (dual).
3. Follow is graph-only (`tangled.graph.follow` ↔ AP `Follow`). Paste `acct:user@instance` or an actor URL; the knot resolves. Reed follow field accepts `acct:@`. No DM collections or AP `ChatMessage`.
4. Ingested Notes look like Reed posts with a tiny federated/attested mark. No Mastodon-specific timeline chrome.
5. `cargo test` for the gateway mapping + attestation gate is green without a live Mastodon; Reed crates keep their existing protocol tests.
