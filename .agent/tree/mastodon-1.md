---
id: mastodon-1
kind: feature
scope:
  - crates/tangled-ap/
  - crates/tangled-knot/
  - crates/tangled/src/foreign.rs
  - crates/tangled/src/http.rs
  - crates/tangled/src/knot.rs
  - apps/reed/core/
  - apps/reed/
acceptance:
  - "cargo test -p tangled-ap -> all tests pass"
  - "cargo test -p tangled-knot --test ap -> all tests pass"
  - "cargo test -p reed-core -> all tests pass"
  - "cargo test -p reed -> all tests pass"
status: implemented
evidence: "cargo test -p tangled-ap: 4 passed; cargo test -p tangled-knot --test ap: 6 passed; cargo test -p reed-core: 16 passed; cargo test -p reed: 25 passed"
---

# Mastodon / ActivityPub gateway on tangled-knot

Same process as the knot. Mapping crate `tangled-ap` is used only by `tangled-knot`. Reed stays a Tangled client (no Mastodon REST/OAuth). Do not edit `apps/loomed/**`.

## Pins

- Resolve XRPC: `GET /xrpc/tangled.ap.resolve?resource=` → `{ did, actor, handle, displayName? }`. MockKnot implements it (no network): `acct:user@host` → `https://host/users/user` → deterministic hosted DID (`foreign::activitypub_actor_identity`).
- Hosted AP DID seeds: BLAKE3(`tangled-ap-ed\0{actor}`) / BLAKE3(`tangled-ap-x\0{actor}`).
- Inbox: draft-cavage HTTP Signatures `rsa-sha256` + `Digest: SHA-256=`. Embedded `actor.publicKey` is enough (tests); live fetch not required in v1.
- Ingest: `sys.foreign` attested + mapped record via `KnotEngine::ingest_record` (not `PostApi`). rkey = `import_rkey("activitypub", id)`.
- Map: Note.content→text, inReplyTo→reply (mapped CID if known), attachments→lossy; Like→`pub.reed.like`; Announce→`pub.reed.boost`; Follow→`tangled.graph.follow`.
- Reed `parse_peer`: `acct:user@instance`, `@user@instance`, `user@host`, `https://` actor URLs. `resolve_peer` calls `ap_resolve` for those.
- `Post.attested` from replica `sys.foreign` (`protocol=activitypub`, `attested=true`, `mapped` or same rkey). UI: tiny `federated` mark.
- Follow chrome: Search `acct:user@instance` field → `follow_resource` → knot resolve → `tangled.graph.follow`.

## Out of scope (v1)

DMs, Mastodon C2S REST, live Mastodon in CI, RFC 9421-only signatures (draft-cavage is what Mastodon speaks).
