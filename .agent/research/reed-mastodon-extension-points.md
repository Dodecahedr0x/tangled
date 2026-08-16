# Where can a Mastodon/ActivityPub gateway plug into Reed and tangled-knot today?

## Answer
**Ingress:** a separate crate writes attested `sys.foreign` + mapped `pub.tangled.post` via `KnotClient::put_record` into `KnotEngine`. Reed `Sync` already `get_head`/`get_repo`s and `apply_record`s any collection; timelines hydrate only `pub.tangled.post`. Do not extend `PostApi`.

**Tangled-only:** identity/sign/CID, `pub.tangled.post` / `tangled.graph.*`, `foreign::{canonical_foreign_id,import_rkey,activitypub_import_attested,verify_ap_proof}`, KnotEngine AP attestation gate, XRPC describe/getHead/getRepo/putRecord. No parallel post format.

**Separate crate:** AP C2S/S2S, WebFinger, HTTP Message Signatures, Note↔post mapping, advertise `activitypub` on `describe.protocols`. `tangled-knot` is XRPC+WS only. Reed has no AP code.

## Citations
- `apps/reed/core/src/sync.rs:52-112` — poll `get_head`/`get_repo` → `apply_record`
- `apps/reed/core/src/replica.rs:93-158,286-335` — generic store; timelines = `POST` only
- `apps/reed/core/src/post.rs:8-86` — `PostApi` = `pub.tangled.post`/`media`
- `apps/reed/core/src/wire.rs:6-14` — no `sys.foreign`
- `apps/reed/core/src/outbox.rs:29-42` — drain `put_record`
- `crates/tangled-knot/src/lib.rs:43-48,117-133` — XRPC fallback → `engine.handle`
- `crates/tangled/src/knot.rs:401-417,676-708` — XRPC + reject unattested AP
- `crates/tangled/src/foreign.rs:6-87` — AP IRI, rkey, attested, proof
- `crates/tangled/README.md:131` — HTTP Message Signatures = gateway product
- `spec/16-interoperability.md:129-133,152` — gateway knot; clients surface `attested`
- `.agent/brief.md:15-16` — protocol via `tangled` only

## Confidence
high — every load-bearing claim is cited from this tree.

## Could not verify
- Wrap `tangled-knot` Router vs sidecar (no gateway crate exists).
- Reed UI for `attested` (spec MUST; no field).
