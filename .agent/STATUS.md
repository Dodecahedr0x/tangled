# Status

Weft explorer shipped (this session): `apps/weft` + `GET /xrpc/tangled.node.stats`.
cargo nextest -p tangled -p tangled-knot -p weft-core -p weft: 203 passed.

Phase: 1 — Mastodon gateway

M7 wrap (same process, extra AP routes on tangled-knot). M8 attested UI (`federated` mark).
Research folded: mastodon-ap-spec, mastodon-client-vs-s2s, reed-mastodon-extension-points.

mastodon-1 implemented
- `crates/tangled-ap` — cargo test -p tangled-ap: 4 passed
- `crates/tangled-knot --test ap` — 6 passed (inbox Create/Follow/Like/Announce + describe)
- Reed `parse_peer` acct:/https; `tangled.ap.resolve` on MockKnot; attested badge
- cargo test -p reed-core: 16 passed; cargo test -p reed: 25 passed; clippy -D warnings clean

Reed run (prior): Phase 5 complete
1.1 done — cargo test -p reed-core: 6 passed — A21,A22,A23,A24
1.1 implemented — cargo test -p reed-core: 9 passed — A27,A28
1.2 done — cargo test -p reed: 18 passed — A20
1.2 defect-fix done — cargo test -p reed: 19 passed; cargo test -p reed-core: 9 passed — A26
Gate 1b closed: Q21 max 4 mixed media; Q22 quote via envelope ref.
1.1 review passed — cargo test -p reed-core: 9 passed
1.2 review passed — cargo test -p reed: 19 passed; clippy -D warnings clean
1 review passed — cargo test -p reed-core: 9 passed; cargo test -p reed: 19 passed
A23 decided — rg -n aliases apps/reed/core/src/replica.rs
A24 decided — rg -n 'deleted.: true' apps/reed/core/src/graph.rs apps/reed/core/src/social.rs
A25 decided — rg -n 'path = "core"' apps/loomed/Cargo.toml && rg -n 'crates/tangled' apps/loomed/core/Cargo.toml
A26 decided — rg inbound_social + SocialKind::Follow; no text.contains(self_did)
A27 decided — rg -n 'post_body' apps/reed/core/src/post.rs apps/reed/core/src/replica.rs
A28 decided — rg -n 'fn with_remote|fn client_for|follow_knots' apps/reed/core/src/sync.rs
