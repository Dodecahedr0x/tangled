# What does Tangled v1 specify for ActivityPub / Mastodon interop, and what would a Reed Mastodon gateway reuse vs invent?

## Answer
Reuse spec 16 + `crates/tangled::foreign`: `sys.binding` (AP `attested: true`, actor URI, well-known JSON or HTTP Message Sig), `sys.foreign` (`foreign_id` = object `id` IRI; rkey = BLAKE3(`protocol||0x00||id`)), types `pub.tangled.post`↔`Note`, follow↔Follow, block↔Block. C2S/S2S facade: outbox = public posts as Notes; inbox = envelope + optional mapped `pub.tangled.post`; dual-publish updates envelope only. Knots reject unattested AP; advertise `describe.protocols`. Badge host-attested. Reed posts are `pub.tangled.post`.

Invent: inbox/outbox/actor/WebFinger HTTP, HTTP Message Signatures (crate: gateway product), field-level Note JSON-LD, Mastodon API, and `pub.reed.*` like/boost/mute/profile — not in `translations.yaml`. Reed and `tangled-knot` have no AP routes.

## Citations
- spec/16-interoperability.md:45-52,62-94,116-133,149 — 2026-08-16
- spec/20-use-cases.md:69-81; spec/06-data-model.md:106-107 (`sys.foreign` rkey TID vs spec 16 BLAKE3)
- spec/02-conformance.md:27,49; spec/interop/translations.yaml:4-37
- crates/tangled/src/foreign.rs:28-87; knot.rs:676-707; README.md:131
- crates/tangled-knot/README.md:45-47

## Could not verify
- Field-level Note mapping (`content`, `inReplyTo`, attachments)
- Whether `gateway.ap` includes Mastodon client API vs W3C AP only
- Mapping for `pub.reed.like` / `boost` / `mute` / `profile`
- AP HTTP routes in `tangled-knot/src` (grep found none)

## Confidence
high for envelopes/facade/crate limits (cited); not high for Reed product mapping — no AP table for app collections.
