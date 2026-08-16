# What is the current (2026) recommended way to build a well-tested, DRY, Dioxus app (web + desktop, optionally mobile) in this Cargo workspace?

## Answer
Pin **dioxus 0.7** (crates.io max_stable **0.7.10**, 2026-07-30). Do not use **0.8.0-alpha.1**. Depend on `dioxus` only — platforms are Cargo features (`web`/`desktop`/`mobile`/`router`), not direct `dioxus-desktop`/`dioxus-web` deps. Skip `fullstack` unless you need Axum SSR/server functions; a knot client is client-only.

Add one workspace member (e.g. `crates/app`) depending on `tangled`. Share **one UI crate** with:

```
[features]
default = ["web"]
web = ["dioxus/web"]
desktop = ["dioxus/desktop"]
mobile = ["dioxus/mobile"]
```

`dx serve --web|--desktop|--ios|--android` enables the matching feature. Optional `dx new` **Workspace** template splits crates per platform (extra complexity).

- **Route:** `dioxus` feature `router`; `#[derive(Routable)]` + `Router::<Route> {}` (web URL / native in-memory history).
- **State:** signals, context, global signals (`dioxus-signals`).
- **Style:** HTML/CSS on all targets; Tailwind if `tailwind.css` at crate root (`dx` runs Tailwind); `asset!()` + `document::Stylesheet`.
- **Test:** `cargo test`; snapshots via `dioxus-ssr::render_element`; hooks via `VirtualDom`; web E2E Playwright + `dx serve`. No `dioxus-test` crate on crates.io.

Limits: web=WASM (no native IO); desktop=WebView, native Rust (`web-sys` not portable); mobile=desktop webview wrapper, no native Android widgets; `dx bundle` host OS/arch only; WSL desktop hard; Blitz/native experimental. This repo has **no** dioxus dep (`rg dioxus` empty).

## Citations
- `curl crates.io/api/v1/crates/dioxus` → max_stable=0.7.10, newest=0.8.0-alpha.1, default_version=0.7.10 — pin 0.7
- same for `dioxus-desktop`/`dioxus-cli`/`dioxus-ssr`/`dioxus-router` max_stable=0.7.10
- `curl crates.io/.../dioxus-test` and `dioxus-testing` → 404 — no first-party test crate
- https://docs.rs/dioxus/0.7.10/dioxus/ — features `web`/`desktop`/`mobile`/`fullstack`/`server`/`router`; `launch` from features
- https://dioxuslabs.com/learn/0.7/tutorial/new_app/ (Stable 0.7.0) — templates Bare-bones/Jumpstart/Workspace; feature flags; Tailwind auto
- https://dioxuslabs.com/learn/0.7/essentials/fullstack/project_setup/ — fullstack = client+server binaries; Workspace template for per-platform crates
- https://dioxuslabs.com/learn/0.7/tutorial/routing/ — `features = ["router"]`, Routable, cross-platform
- https://dioxuslabs.com/learn/0.7/guides/testing/web — ssr snapshots, VirtualDom hooks, Playwright E2E
- https://docs.rs/dioxus-ssr/0.7.10/dioxus_ssr/ — `render`/`render_element` for HTML asserts
- https://dioxuslabs.com/learn/0.7/beyond/project_structure/ — desktop/mobile=webview; native/Blitz experimental; playwright-tests unpublished
- https://dioxuslabs.com/learn/0.7/guides/platforms/web — WASM crate limits; web best-supported
- https://dioxuslabs.com/learn/0.7/guides/platforms/desktop — WebView/wry; Rust native; `web-sys` not portable
- https://dioxuslabs.com/learn/0.7/guides/platforms/mobile — webview or experimental WGPU; no native Android widgets
- https://dioxuslabs.com/learn/0.7/getting_started — Linux webkitgtk; WSL desktop “tricky”; iOS needs macOS+Xcode
- 0.7 llms-full.txt bundling — `dx bundle` native platform/arch only; mobile signing not in dx
- 0.7 llms-full.txt fullstack-desktop — native fullstack: no hydration/SSR; `set_server_url`; stable `#[post]` paths
- `Cargo.toml` members `crates/tangled`, `crates/tangled-knot` — no dioxus
- `rg dioxus` in repo → no matches

## Confidence: medium
Version/features/layout/router/state/styling/limits are cited from crates.io + 0.7 book; would rise to high if the current `dx new` Workspace crate tree were fetched and 0.7.10 vs book-labeled 0.7.0 drift were confirmed.

## Could not verify
- Exact `dx new` Workspace member names/paths: GitHub `DioxusLabs/dioxus-template` is old per-platform folders (Web/Desktop/Fullstack); CLI template sources under `packages/cli/assets/templates` 404’d.
- wasm-bindgen-test as a recommended path: not in the 0.7 testing guide (only `cargo test`, ssr, Playwright).
- Desktop/mobile E2E: official testing page only documents Playwright for web.
- Whether book pages labeled “Stable 0.7.0” differ from crate 0.7.10 APIs.
- 0.8-alpha recommended layout: not researched; do not use until stable.
