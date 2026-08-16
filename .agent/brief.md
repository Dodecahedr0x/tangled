# Brief — Reed

## What we're building

**Reed** is a polished Dioxus 0.7 Twitter clone on Tangled v1 (`crates/tangled`). HTTP client to a user-supplied knot URL. Tests use MockKnot.

A person generates a `did:sol` identity (Ed25519 + X25519) or imports seeds; on web they may connect a matching wallet via dioxus-solana to sign. They compose public `pub.tangled.post` records (text up to 32 KiB, `pub.tangled.media` + `putBlob` for images/video, replies via body `reply`). Follow by DID or handle (`tangled.graph.follow`). Block via spec `tangled.graph.block`. Likes (`pub.reed.like`), boosts (`pub.reed.boost`), mute (`pub.reed.mute`), and profile (`pub.reed.profile`: display name, bio, avatar) are application collections published with `sys.schema` (Last-ts-wins). Quote is a new public post that references the original (wire form Q22). Home is client-assembled: own `tangled.graph.follow` then `getHead`/`getRepo` per author, filter `pub.tangled.post`. Discover is public posts the knot/replica knows. Search is local over the replica plus already-fetched Discover posts. Mentions (`@handle`) link and notify. Notifications: reply, like, boost, follow, mention — in-app plus local OS. Own posts can be edited (last-ts-wins) and deleted. History is a local replica with an outbox; knot-down is cached read-only plus a banner and queued posts. wasm `persist_keys` is a no-op (Loomed). UI `drain_outbox` does not SIWS; tests use `MockKnot::hosting`.

Platforms: web, desktop, iOS, Android. Domain crate `apps/reed/core` (no Dioxus). UI crate `apps/reed`.

Visual: same dark metal as Loomed (suite). CSS tokens: bg `#12141a`, sidebar `#0c0e12`, copper `#c47a4a`, text `#e6e8ee`, muted `#9aa3ad`, radius 2px, Fira Sans.

## Constraints

- Protocol via `crates/tangled` only. No parallel post format.
- Protocol client patterned on Loomed (identity, knot HTTP, replica, outbox, handle resolve). Do not copy Loomed chat circles, MLS, HPKE, or chat receipts.
- Do not add spec collections for likes/boosts/mute/profile. Use `sys.schema` + `pub.reed.*`. Use spec `tangled.graph.block` for block.
- Video is putBlob + client mime + in-app player; no transcoding. Honor `describe.maxBlob`. Skip types the platform cannot play.
- Workspace: add `apps/reed` and `apps/reed/core` members; dioxus `=0.7.10`; feature-gate `launch`; default CI has no desktop/wasm features.
- Do not overwrite spec-site `PRODUCT.md` / `DESIGN.md` or `apps/loomed`.
- No APNs/FCM. No fake public network.

## Success criteria

1. `cargo test -p reed-core` green: generate/import identity, MockKnot post + follow + reply + quote + like/boost app records, image/video blob, local search, outbox drain, handle parse, mute/block filter, edit/delete.
2. `cargo test -p reed` green for SSR/view snapshots that can run without a window.
3. First-run: identity, knot URL, empty Home + Discover in the dark-metal world.
4. Knot down: banner + cached timeline. Offline search works.
