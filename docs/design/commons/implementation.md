# Commons implementation

The selected design now uses real Rust/Dioxus views and ordinary provider HTML forms. The earlier visual prototype remains reference material, not runtime code. Synthetic content is confined to test-only capture helpers.

- Shared foundation: `apps/commons.css`; app-specific content rules remain in each app stylesheet.
- Cross-application links: optional build-time `TANGLED_REED_URL`, `TANGLED_LOOMED_URL`, and `TANGLED_WEFT_URL`; `TANGLED_KNOT` supplies the hosted account destination. Unconfigured destinations are not rendered as dead links. Local navigation remains the existing router.
- Provider retains its normal routes, methods, field names, escaping, custody, and migration behavior.
- Documentation website retains its existing visual identity.
- Real-view captures: set `TANGLED_DESIGN_CAPTURE_DIR` and run `cargo test -p reed -p loomed -p weft capture_commons`; provider uses `cargo test -p tangled-provider render_design_capture_fixtures`.

The source branch is based on the published `master` development branch, which contains the provider and imports prerequisites. The older default `main` does not contain the provider. This change targets `master` without promoting unrelated development history into `main`.
