# What public API of `crates/tangled` would a UI call for identity, knot, chat, caps, seal, and sync?

## Answer
Root re-exports (`lib.rs`): `Identity`, `Did`, `KnotClient`, `MockKnot`/`KnotEngine`, `Capability`, `Invocation`, `CircleState`, `Distributor`, `MembershipPlan`, `Visibility`, `Cid`, `Car`, `Mst`, `LivePage`/`PageRecord`/`apply_page`, `OverlayCount`, `BlobAuth`/`BlobRef`/`getblob_auth`, `Error`/`Result`. Call `http`/`record`/`hpke`/`capability`/`handle`/`sync` via `pub mod`. No `mock_knot.rs` — `MockKnot = KnotEngine` in `knot.rs`.

- **Identity:** `Identity::generate` / `from_seeds` / `to_seeds`. No keystore.
- **Knot:** `KnotClient::mock` / `mock_shared` / `http(url)` + `with_cap_header`; `MockKnot::hosting(did)`.
- **Publish chat:** app JSON (`collection: "pub.tangled.chat"`) → `record::sign_json_record` → `put_record(&PutRecordRequest)`. No `publish_chat`. Rkeys: `tid_now`.
- **List/fetch:** `get_record(did, coll, rkey)`. `list_records` is MockKnot-only (not in OpenAPI); live knots: `get_repo` CAR. No conversation type.
- **circle/read:** construct `Capability` + `.sign`; `circle_resource_uri`; `Invocation::new`+`.sign`; `cap_authorization_header`. Test `issue_cap` is **not** crate API.
- **1:1 seal:** `hpke::seal_record`/`open_record`; AAD `unsigned_without_body_and_sig`. Circle: `CircleState` + `epoch_aead_seal`. MLS payloads opaque.
- **Subscribe/sync:** `subscribe`/`loom_firehose` are GETs; MockKnot errors “websocket upgrade is not implemented”. OpenAPI is 101 WS. Poll: `get_head`/`get_repo`/`notify`. In-proc drain: `take_events`.

App-layer: key storage, envelope JSON, conversation/thread derived state, WS client, DNS/HTTP for handles (`handle::verify_handle` needs caller-supplied TXT), RFC 9420 MLS.

## Citations
- `crates/tangled/src/lib.rs:10-43` — `pub mod` + crate-root re-exports
- `crates/tangled/src/identity.rs:109-161` — `generate`/`from_seeds`/`to_seeds`; no persistence
- `crates/tangled/src/http.rs:274-547` — `KnotClient::mock`/`http`/`put_record`/`get_record`/`list_records`/`subscribe`/`get_repo`
- `crates/tangled/src/http.rs:358` — `list_records` “in-process stand-in for a getRepo CAR”
- `crates/tangled/src/knot.rs:77-78,152-154,224-227` — `MockKnot` alias; `take_events`; WS unimplemented
- `crates/tangled/src/record.rs:40-67,101-108` — `sign_json_record`, `unsigned_without_body_and_sig`
- `crates/tangled/src/hpke.rs:203-210` — `seal_record`/`open_record`
- `crates/tangled/src/capability.rs:28-118,338-344,444-501,643` — `Capability::sign`, `circle_resource_uri`, `Invocation`, `cap_authorization_header`
- `crates/tangled/src/circle.rs:90-119` — `CircleState::create`; MLS opaque
- `crates/tangled/src/handle.rs:30-36` — verify only; no DNS fetch
- `crates/tangled/tests/common/mod.rs:61-140` — `issue_cap`/`publish` are **test** helpers, not crate API
- `crates/tangled/tests/e2e.rs:143-210` — chat + `circle/read` + HPKE 1:1 call pattern
- `spec/http/openapi.yaml:219-227` — `subscribe` is 101 WS; no `listRecords`
- `crates/tangled/README.md:104-106` — WS subscribe/firehose unimplemented

## Confidence: high
Would drop if `lib.rs` re-exports or OpenAPI methods change; every load-bearing claim is cited from this tree.

## Could not verify
- Live HTTP knot `listRecords` support: OpenAPI omits it; no live knot in-repo (`brief.md`).
- `KnotClient::http` WS 101: `http_send` is GET/POST only (`http.rs:575-620`); no 101 handling found.
