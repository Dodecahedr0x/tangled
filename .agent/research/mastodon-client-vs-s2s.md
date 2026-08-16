# For Reed ↔ Mastodon: client API+OAuth vs AP S2S/C2S gateway?

## Answer
**(1)** Reed is a Mastodon app: `POST /api/v1/apps`, user OAuth (`GET /oauth/authorize`, `POST /oauth/token`), `Authorization: Bearer`. Per-instance; not Tangled.

**(2)** A `gateway.ap` knot federates S2S; Reed stays on Tangled HTTP. Inbox → `sys.foreign` + attested `pub.tangled.post`. AP has no author key.

Client REST (user token unless noted): home `GET /api/v1/timelines/home` (`read:statuses`); public `GET .../public`; profile `GET /api/v1/accounts/:id` (public) + `.../:id/statuses`; follow `POST .../:id/follow` (`write:follows`); post/reply `POST /api/v1/statuses` (`write:statuses`, reply=`in_reply_to_id`); favourite `POST .../:id/favourite` (`write:favourites`); boost `POST .../:id/reblog` (`write:statuses`).

S2S (HTTP Signatures; `Digest` on POST; RFC9421 ≥4.5): no home feed. `Create{Note}` / `Follow` / `Like` / `Announce` → `POST /users/:name/inbox`. Reply = `inReplyTo`. Profile = GET actor `/users/:name`.

Mastodon clients use REST, not AP C2S. Tangled C2S is an outbox facade.

## Citations
- docs.joinmastodon.org: `/client/authorized/`, `/methods/{timelines,statuses,accounts}/`, `/spec/{activitypub,security}/` — fetched 2026-08-16
- `spec/16-interoperability.md:131-133`, `spec/02-conformance.md:27` — `gateway.ap`, attested
- https://github.com/mastodon/mastodon/issues/10520 — C2S open — 2026-08-16

## Confidence
medium — REST/S2S/auth from docs; “no Mastodon C2S” is GitHub.

## Could not verify
- Official docs never say C2S is unimplemented.
- `sharedInbox` path as a documented method.
- Tangled gateway C2S auth (spec: facade).
- Like/Announce → `pub.reed.*` (absent from `spec/interop/translations.yaml`).
