# tangled (Rust reference client)

Library (plus a thin `tangled` CLI) implementing the Tangled v1 **client** surface: identities, signatures, CIDs, records, MST proofs, capabilities, page CRDT + compaction, circle dual-stack, HPKE, MLS (RFC 9420), SIWS, handle resolution, foreign_id rkeys, CARv1, and a typed knot HTTP API.

This is **not** a knot server. The knot HTTP process is [`tangled-knot`](../tangled-knot/).

## Complete vs opaque

| Area | Status |
| --- | --- |
| `did:sol`, Ed25519, separate X25519 agreement; generative vs operational DID document | Complete |
| Solana off-chain envelope sign/verify | Complete |
| CIDv1 dag-cbor/raw + blake3, multibase `b` | Complete |
| DAG-CBOR (RFC 7049 §3.9 map key order) | Complete |
| MST insert + inclusion proof (string-CID encoding matches vectors) | Complete |
| Capabilities, attenuation, `circle/read`, DAG-CBOR invocations, `max_uses` count, 60s clock skew | Complete |
| Page RGA + LWW apply, compact merge of stale concurrent ops | Complete |
| HPKE RFC 9180 base mode (X25519 / HKDF-SHA256 / ChaCha20-Poly1305) | Complete |
| Epoch AEAD + sealed rkey wrap | Complete |
| Circle dual-stack / never-downgrade / agreement-rotation rewrap list | Complete |
| MLS RFC 9420 Welcome/Commit (OpenMLS, ciphersuite `0x0001`); KeyPackage publish (`sys.mls.keypackage`) | Complete |
| Canonical `foreign_id` + BLAKE3 rkey | Complete |
| HTTP types for every OpenAPI method + knot engine; `getRepoAuth` `since` in query or body; signed Commit `prev` on `putRecord` | Complete |
| SIWS message + verify (cookie is set by the knot) | Complete |
| Handle TXT + well-known + SNS resolve (injectable; live DoH/HTTPS/`SOLANA_RPC`) | Complete |
| CARv1 getRepo / import round-trip | Complete |

## Build and test

From the repository root (workspace) or `crates/tangled`:

```bash
cargo test
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Unit tests do not use the network. Spec vectors are read from `docs/spec/vectors/*.json`. Live SNS JSON-RPC is `#[ignore]` and needs a local validator (never mainnet):

```bash
# Agave v4.2.1 in CI (`solana-test-validator`). Local 4.0+ works.
scripts/sns-validator.sh --bg
SOLANA_RPC=http://127.0.0.1:8899 cargo test -p tangled --test sns_rpc -- --ignored
```

`scripts/sns-validator.sh` is `solana-test-validator --reset --quiet --bind-address 127.0.0.1` plus `--account-dir crates/tangled/fixtures/sns` (Bonfida 96-byte registries for `alice.sol` / `bob.sol`). A bare `solana-test-validator --reset` has no SNS accounts. Regenerating fixtures: `WRITE_SNS_FIXTURES=1 cargo test -p tangled --test sns_rpc write_sns_fixtures -- --exact`.

```bash
# Multi-node in-process e2e (no sockets, no public internet)
cargo test --test e2e --test e2e_protocol --test siws
```

Each e2e node is a distinct identity plus an in-memory knot. Clients pull from **other** knots via `KnotClient::mock_shared`; visibility and capabilities are enforced at the serving node.

```bash
# CLI
cargo run -p tangled -- identity
```

### SIWS / MLS / resolve

```bash
cargo test -p tangled --test siws
cargo test -p tangled --lib mls::
cargo test -p tangled handle
```

SIWS: `KnotClient::session_challenge` / `session_verify`, then `with_session_cookie`. MLS: `mls::MlsParty::from_identity` / `MlsSession::add` / `join_welcome`; leaf HPKE key is the Tangled `agreement` public key; exporter label `tangled.epoch`. Handle resolve: inject `HandleResolver.txt` / `well_known` / `sns` in tests; production sets `live: true` and `SOLANA_RPC` for SNS. Live RPC coverage is `cargo test -p tangled --test sns_rpc -- --ignored` against `scripts/sns-validator.sh`.

## Layout

```
crates/tangled/src/
  lib.rs identity.rs sign.rs cid.rs dagcbor.rs record.rs
  mst.rs capability.rs circle.rs mls.rs hpke.rs page.rs car.rs
  foreign.rs handle.rs session.rs sync.rs http.rs knot.rs error.rs constants.rs
```

In-process e2e lives in `tests/e2e.rs` and `tests/e2e_protocol.rs` with a shared harness in `tests/common/mod.rs`.

## Coverage map

| Spec function | Test(s) |
| --- | --- |
| did:sol encode/decode, clusters | `identity::did_roundtrip`, `identity::cluster_dids_roundtrip` |
| Generative vs operational DID `keyAgreement` | `identity::generative_has_no_key_agreement`, `identity::operational_includes_sys_id_agreement`, `generative_then_operational_did_document` |
| rkey / page slug | `identity::rkey_and_slug` |
| Identity ≠ agreement; ECDH of Ed25519 identity rejected | `identity::agreement_is_not_ed25519`, `hpke::reject_ecdh_of_identity_key` |
| Off-chain envelope + negative sig | `vector_solana_signature`, `sign::envelope_layout`, `envelope_rejects_raw_cbor_sign_helper` |
| Device-key-only MUST NOT rewrap | `circle::device_key_only_must_not_rewrap` |
| Agreement rotation within/outside skew | `circle::agreement_rotation_within_and_outside_skew` |
| Handle TXT + well-known + alsoKnownAs mismatch | `vector_handle_dns`, `handle_txt_and_well_known_fixtures`, `handle::also_known_as_mismatch`, `handle_one_sided_resolve_fails` |
| SNS `.sol` (injected registry; live JSON-RPC when `SOLANA_RPC`) | `handle::sns_injected_resolves_and_one_sided_fails`, `sns_handle_resolve_http`, `sns_rpc::live_sns_tangled_field_via_rpc` |
| vis public/cap/sealed; unknown rejected | `vis_enum_matches_wire`, `record::unknown_vis_rejected`, `unknown_vis_rejected_on_publish` |
| Core collections create/sign/CID/fetch | e2e public post, follow, chat, page, `labels_and_core_collections`, `album_blob_and_graph_block`, `foreign_id_import_idempotent`, `sys.id` in `migration_location_and_device_revoke` |
| Page RGA ins/del, LWW, tombstones in atom cap | `vector_page_crdt_concurrent`, `page::tombstoned_atoms_remain_in_rga` |
| Compact snapshot + frontier; stale concurrent op | `vector_page_crdt_compact`, `wiki_concurrent_crdt_then_stale_compact` |
| `PAGE_COMPACT_MIN_OPS` / interval admission | `page::compact_admission_min_ops_and_interval`, `compact_admission_rate_limited` |
| Last-ts-wins MUST NOT compact | `record::last_ts_wins_must_not_compact`, `compact_admission_rate_limited` |
| Compact `I(C)` required; lying snapshot rejected | `page::compact_missing_included_block_is_rejected`, `page::compact_with_fetchable_included_is_accepted`, `page::lying_compact_snapshot_is_rejected` |
| MST insert, inclusion, missing-key reject | `vector_mst_proof`, `mst::empty_and_single`, `mst::missing_key_proof_rejected` |
| ConcurrentHeads + signedBy on one knot | `shared_repo_signed_by_concurrent_heads` |
| Signed Commit `prev` chain; `getHead.commit` is Commit CID | `record::signed_commit_roundtrip_and_prev`, `signed_commit_prev_chain_and_mismatch` |
| CAR getRepo / import two knots | `car_get_repo_roundtrip_two_knots`, `car::roundtrip_small_repo` |
| Incremental getRepo `since` | `get_repo_since_is_incremental` |
| `getRepoAuth` `since` in JSON body | `get_repo_auth_since_in_json_body`, `get_repo_auth_honors_body_since` |
| KeyPackage publish / add from bytes | `mls::add_from_published_keypackage_bytes` |
| getBlob `Content-Length` + media type | `blob_get_sends_length_and_media_type` |
| public / cap / sealed fetch; sibling denied | `public_post_not_on_bob_until_pull`, `capability_gated_fetch_and_sibling_denied` |
| Attenuation, unknown cmd/caveat, expired | `vector_capability_chain`, `capability::unknown_command_rejected`, `expired_and_revoked_cap` |
| Clock-skew window on `nbf` / `exp` / `valid_until` | `capability::clock_skew_window_on_nbf_exp_and_valid_until` |
| `max_uses` counted per leaf CID | `max_uses_overflow_is_403` (knot HTTP) |
| `circle/read` membership | `circle_read_membership_grant`, `vector_circle_read_auth` |
| Invocation DAG-CBOR; JSON and mutated rejected | `invocation_dagcbor_and_mutated_rejected` |
| Stolen read cap cannot write | `stolen_read_cap_cannot_write` |
| HPKE 1:1, epoch wrap, sealed rkey | `vector_hpke_sealed`, `sealed_hpke_third_party_cannot_read`, `hpke::sealed_rkey_is_hex_and_stable` |
| Dual-stack / never-downgrade / shrink MLS | `vector_circle_grow_dual_stack`, `circle_grow_dual_stack_same_id`, `circle::never_downgrade` |
| MLS Welcome decrypt + exporter; leaf HPKE = agreement | `mls::create_add_welcome_exporter`, `mls::leaf_hpke_is_agreement_and_hpke_wrap_target` |
| getBlob auth | `vector_getblob_auth`, `sealed_blob_requires_cap` |
| OpenAPI methods vs knot engine | `mock_knot_describe_head_put`, `ap_import_attested_and_get_blocks` |
| SIWS challenge/verify/cookie | `siws_happy_path_cookie_put`, `siws_bad_signature`, `siws_expired_challenge`, `siws_cookie_required_without_hosted_identity_key` |
| queryBacklinks empty ≠ zero comments | `sync`/`http` overlay helpers, `annotation_backlinks_and_empty_overlay` |
| Looms `type=ref` | `annotation_backlinks_and_empty_overlay` |
| Canonical foreign_id (Nostr/AT/AP/FC) | `foreign::canonical_ids_all_protocols`, `vector_binding_nostr` |
| Idempotent sys.foreign rkey | `foreign_id_import_idempotent` |
| AP attested imports | `foreign::activitypub_must_be_attested`, `ap_import_attested_and_get_blocks` |
| AP HTTP Message Signatures (RFC 9421) | `httpsig::valid_signature_accepted_mutated_body_rejected`, `foreign::activitypub_httpsig_present_must_verify`, `ap_httpsig_mutated_body_rejected` |
| Account migration CAR + location + revoke | `migration_location_and_device_revoke` |
| Graph unfollow/block tombstone | `album_blob_and_graph_block` |
| WebSocket subscribe/firehose | In-process engine returns transport error; `tangled-knot` implements a real 101 upgrade |

## Remaining invariants (not limits)

- **MLS Welcome init key:** RFC 9420 requires KeyPackage `init_key` ≠ leaf HPKE. The leaf encryption key is the Tangled X25519 `agreement` key (HPKE wraps and MLS path secrets). Welcome is sealed to the OpenMLS-generated init key. Ciphersuite `0x0001`, basic credential = 32-byte Solana pubkey, exporter label `tangled.epoch` match spec 05/10. Implementations MUST NOT derive X25519 from the Ed25519 identity key.
- **`listRecords`** is a convenience on the engine; the HTTP knot returns `MethodNotImplemented` (use `getRepo`). This is an intentional spec forbid.
- **MLS wrapping vector** `docs/spec/vectors/mls-welcome-commit.json` is wrapping-only; live RFC 9420 bytes are produced by OpenMLS in `cargo test -p tangled --lib mls::` (Welcome hex is not frozen; not deterministic).
- **WebSocket** `subscribe` / `loom.firehose` are implemented in `tangled-knot`. The in-process engine used by unit tests still returns a transport error (no socket). The HTTP knot persists `since` replay.
- **Live SNS** calls Solana JSON-RPC only when `HandleResolver.solana_rpc` / `SOLANA_RPC` is set. Default `cargo test` injects `HandleResolver.sns` and never hits the network. Ignored tests in `tests/sns_rpc.rs` call real `getAccountInfo` against `solana-test-validator` (CI and `scripts/sns-validator.sh`); they refuse `mainnet-beta` unless `SNS_ALLOW_MAINNET=1`.
