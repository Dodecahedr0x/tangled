# How is the Cargo workspace / Dioxus app crate wired, and what must `apps/<name>` copy vs Loomed-specific?

## Answer
Members are explicit: `crates/tangled`, `crates/tangled-knot`, `apps/loomed`, `apps/loomed/core`. No `workspace.dependencies`, no `Dioxus.toml`.

**Copy** (else CI/`just clippy`/`just test` miss or fail the crate):
- Add `apps/<name>` and, if split, `apps/<name>/core` to root `members`.
- Pin `dioxus = "=0.7.10"` + `features = ["router"]`; `web`/`desktop`/`mobile` → `dioxus/web|desktop|mobile`; `default = []`.
- `[[bin]] required-features = ["desktop"]` and feature-gate `dioxus::launch` so default `cargo test --workspace` / `cargo clippy --all-targets` (CI: no features, no `dx`, no wasm) compile.
- `dioxus-ssr = "=0.7.10"` + VirtualDom tests (no window).
- Unique package names; `publish = false`; edition 2021 / Apache-2.0.
- Optional: `just <name>`; wasm32 `dioxus-solana` if web wallet.

**Do not copy:** `loomed-core`, `notify-rust`, `pub.loomed.receipt`, Fira/`app.css`, `LOOMED_HOME`/`.loomed`, chat routes, `just loomed` / `just test-live`. CI needs no edit unless adding ignored live tests.

## Citations
- `Cargo.toml:1-8` — four members; no workspace dioxus pin
- `apps/loomed/Cargo.toml:1-35` — dioxus 0.7.10, router, web/desktop/mobile, bin `required-features = ["desktop"]`, notify-rust, wasm32 dioxus-solana, dioxus-ssr
- `apps/loomed/core/Cargo.toml:1-15` — `loomed-core` → `tangled` only
- `apps/loomed/src/main.rs:1-8` — launch gated on web|desktop|mobile
- `apps/loomed/src/lib.rs:39-44,91-92` — `asset!`/`include_str` CSS; `ssr_tests`
- `justfile:18-41` — `test`/`clippy` workspace-wide; `loomed`/`test-live` Loomed-only
- `.github/workflows/ci.yml:32-39` — fmt/clippy/test `--workspace`, no Dioxus features
- `apps/loomed/src/paths.rs:3-11` — `LOOMED_HOME` / `~/.loomed`

## Confidence: high
All load-bearing claims are from files read; would drop if CI later enabled `--features desktop` or `dx`.

## Could not verify
- Linux CI + `--features desktop` (webkitgtk): CI never enables desktop.
- Whether a second `asset!()` crate needs `Dioxus.toml`: none exists for Loomed.
