# Commons Suite Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans for the app integration and a focused provider implementer. Verify the whole branch before merging.

**Goal:** Apply the user-selected Commons design to the real Reed, Loomed, Weft, and hosted provider, then merge the verified implementation.

**Architecture:** Preserve existing Rust/Dioxus views, routes, state, signing, and HTML form handlers. Introduce one shared CSS foundation consumed by the three app style builders, retain only app-specific structural styles, and restyle provider forms in the same family. No prototype/synthetic content ships in production.

**Tech Stack:** Rust, Dioxus 0.7, CSS, Axum server-rendered HTML.

**Spec:** `docs/design/commons/brief.md`; selected visual tokens `docs/design/commons/DESIGN.md` and `tokens.json`.

## Global Constraints

- Keep current protocol, identity custody, storage, URLs, authentication, and signing behavior.
- Native UI sans, evergreen #153e38, leaf selection #d8edc8, canvas #f3f6f5, white surfaces, 8px controls, 10px surfaces.
- No backend operations are replaced by the prototype's simulation handlers.
- Weft remains read-only and advertises its observation scope; Reed public and Loomed private scope stay visible.
- Preserve unrelated uncommitted source work by implementing in an isolated checkout based on origin/master.
- Documentation website retains its existing visual system.

## Review Focus

- Long DIDs and host addresses must wrap without losing the identity or essential actions at 320px.
- Offline content and empty state must keep real retry/settings/navigation available.
- Provider signup/login/migration preserve field names, methods, actions, escapes, and custody disclosures.
- Dioxus links and event handlers remain functional through the layout replacement.
- Production web assets and SSR renders consume the same shared tokens without external font requests.

## Tasks

### Task 1: Real app integration

- [x] Baseline `cargo test -p reed -p loomed -p weft -p tangled-provider --offline`.
- [x] Shared CSS in `apps/commons.css`, consumed by each `app_style()`; retire timetable palette and mobile overrides in the three `assets/app.css` files.
- [x] Update navigation, identity/host anchors, Home hierarchy, public/private labels, and task-specific reading layouts in existing views.
- [x] SSR regression checks for semantic navigation and custody/scope labels; use rendered fixtures for desktop/mobile QA.
- [x] `cargo test -p reed -p loomed -p weft --offline` and build web features.

### Task 2: Provider integration

- [x] Add regression tests for accessible shared shell, custody notice, escaping, and real form contracts.
- [x] Replace provider dark/copper layout with Commons CSS and state-appropriate navigation, preserving handlers and dynamic content.
- [x] Cover signup, login, signed-in account, start move, move code, move in, and moved state.
- [x] `cargo test -p tangled-provider --offline`.

### Task 3: Review, verify, merge

- [x] Generate real Rust-view fixtures with synthetic data confined to test helpers. User explicitly chose automated checks and code review instead of enabling the disabled browser; live desktop/mobile inspection is skipped at that instruction.
- [x] Update app design authority while retaining docs design authority.
- [x] Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, and app web compile checks.
- [x] Independent full-branch code review; all material issues fixed with regression coverage. Visual review skipped at the user’s instruction because browser use is disabled.
- [ ] Commit only redesign changes and fast-forward published development `master` after checking its latest revision and protections, then verify the remote contains the signed commit. Do not reset or stash unrelated source work. The user explicitly authorized merge; `master` is unprotected and contains the required provider code.

## Validation record

Final isolated-tree checks: 419 tests passed, 6 existing live tests ignored; formatting check, all-targets Clippy with warnings denied, and WebAssembly builds for Reed/Loomed/Weft passed. Provider progressive-enhancement script passed an automated event harness. Scoped code-review findings were fixed with red-to-green regression coverage, including replacing Loomed’s local identity without leaving the shared identity label stale.
