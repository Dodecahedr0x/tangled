# How does Loomed talk to Tangled? What should a sibling `apps/` client copy?

## Answer
Split: `loomed` (Dioxus) depends only on `loomed-core`; core depends on `tangled` only. Copy `apps/<name>` + `apps/<name>/core`.

Identity: wrap `Identity::generate`/`from_seeds`/`to_seeds`. Import needs Ed25519+X25519 (`from_solana_json`). `FileKeyStore` → `keys.json` (unix 0600). Onboard generate/import. wasm `persist_keys` is a no-op. `dioxus-solana` (wasm32) is a Wallet Standard signer that matches pubkey to `did:sol:` — not a keystore.

Knot: user URL, default `http://127.0.0.1:3000`, file `knot.txt`. `Sync::http` → `KnotClient::http`; `Sync::mock` → `mock_shared`. Probe: `describe`. Public poll: `get_head` + `get_repo` → CAR. Writes: `Outbox.drain` → `put_record`. Live writes need SIWS (`session_challenge`/`verify` + `with_session_cookie`); UI drain does not SIWS. `MockKnot::hosting(did)` lets that author write without SIWS.

Handles: `parse_peer` (`did:sol:` vs handle) → `identity_resolve` + `handle::parse_well_known`.

Tests: default `MockKnot::hosting`; ignored `knot_live` vs `tangled-knot`.

**Copy for public posts:** `record::sign_json_record`, `vis: "public"`, poll `get_head`/`get_repo`, drain `put_record`.

**Do not copy:** `Chat`, `CircleRef`/`CircleState`, `sys.circle`/`sys.circle.wrap`, `pub.tangled.chat`, `pub.loomed.receipt`, HPKE/MLS/`CIRCLE_MLS_THRESHOLD`, `circle/read`, `get_repo_auth`, `loom_query` for circles, envelope seal.

## Citations
- `apps/loomed/Cargo.toml:25-31` — UI deps `loomed-core`; wasm `dioxus-solana`
- `apps/loomed/core/Cargo.toml:14` — core deps `tangled` only
- `apps/loomed/core/src/lib.rs:1-25` — domain modules; no Dioxus
- `apps/loomed/core/src/identity.rs:21-144` — generate/import/`FileKeyStore`
- `crates/tangled/src/identity.rs:111-159` — `Identity::generate`/`from_seeds`/`to_seeds`/`did`
- `apps/loomed/src/onboard.rs:28-80` — generate/import; wasm wallet slot
- `apps/loomed/src/wallet.rs:6-30` — signer match, not secret store
- `apps/loomed/src/state.rs:9-176` — default knot, probe, persist, drain, poll
- `apps/loomed/src/paths.rs:1-24` — `~/.loomed` keys/knot/replica
- `apps/loomed/core/src/sync.rs:17-136` — `http`/`mock`/`probe`/`poll_once`/`get_repo`
- `apps/loomed/core/src/outbox.rs:14-31` — enqueue + `put_record`
- `apps/loomed/core/src/replica.rs:37-56` — local `replica.json`
- `apps/loomed/core/src/handle.rs:5-24` — `parse_peer` + `identity_resolve`
- `apps/loomed/src/compose.rs:57-74` — resolve handle then open peer
- `crates/tangled/src/http.rs:291-323,350,358,419,438,474,517,573-586` — client + XRPC
- `crates/tangled/src/knot.rs:88,904-917` — `hosting`; write needs host/SIWS/Cap
- `apps/loomed/core/tests/core.rs:88-189` — MockKnot + outbox + `Sync::mock`
- `apps/loomed/core/tests/knot_live.rs:170-187,248-296` — SIWS + live `poll_once`
- `apps/loomed/core/src/chat.rs:12-117` — circles/`pub.tangled.chat`/MLS threshold
- `apps/loomed/core/src/receipts.rs:7-27` — `pub.loomed.receipt`
- `apps/loomed/core/src/envelope.rs:15-77` — sealed HPKE envelope

## Confidence: high
Would drop if loomed-core gains a second client or UI starts calling `tangled` directly.

## Could not verify
- wasm identity persistence besides `persist_keys` no-op: searched `apps/loomed/src` — no localStorage keystore
- whether a shared live knot accepts UI `drain_outbox` without SIWS: UI omits cookie; only `knot_live` SIWS (ignored, not run here)
