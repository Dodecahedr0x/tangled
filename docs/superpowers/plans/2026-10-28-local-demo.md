# Managed Local Demo Implementation Plan

Goal: `just demo` starts the complete local setup with sample data, opens the interfaces and owns cleanup until the user exits.
Architecture: Node 22 standard-library supervisor calls existing binaries/seeders and the same Personal/provider APIs. Each run uses a fresh private directory under .data/local-demo; existing profiles are untouched. Seven default runtime processes: knot, Personal, provider, docs, Reed, Loomed, Weft. Browser openers are short-lived and never own the user's browser.

- [ ] Root: add tested scripts/local-demo.mjs supervisor, just recipe and README usage.
- [ ] Seed agent: scripts/local-demo-seed.mjs and tests/local-demo-seed.test.mjs, only existing Personal/provider APIs and ordinary records.
- [ ] Lifecycle agent: tests/local-demo.test.mjs and tests/fixtures/local-demo-tool.mjs fake tools; validate real child/grandchild cleanup, no killing port occupants, startup failures, EOF/Enter/signals and desktop exit.
- [ ] Integrate, run fake-stack tests plus real services/sample content in no-desktop mode, build desktop binaries and verify their launch/lifecycle when available.
- [ ] Independent review of process ownership and secret/data isolation; fix findings.
- [ ] Preserve current pending main changes, merge master and verify combined files.

Supervisor flags: --no-open (no browser openers), --no-desktop (skip native app build/start), --skip-build (use existing binaries/site deps), --port-offset N (add N to defaults 3000/8788/8787/4321), --data-root DIR (parent of private fresh run-* directory), --help. macOS/Linux process-group ownership; refuse unsupported platforms explicitly. Node and npm already needed for docs; no new npm dependencies.

Prepare with cargo build -p tangled-knot -p tangled-personal -p tangled-provider -p reed-core -p loomed-core -p weft-core --bins; when desktop enabled, build -p reed -p loomed -p weft --features reed/desktop,loomed/desktop,weft/desktop --bins. Set TANGLED_KNOT/TANGLED_PERSONAL_URL during desktop build. npm ci in site when dependencies absent. Resolve target_directory via cargo metadata --offline --no-deps --format-version 1; respect CARGO_TARGET_DIR. Each app profile gets knot.txt for chosen knot.

Readiness is bounded and checks child failure, before any seeding/opening. Reject occupied ports without touching existing listeners. Every spawned build/server/seeder/native/opener uses its own POSIX process group. Cleanup handles Enter, stdin EOF, SIGINT/SIGTERM/SIGHUP, failures and desktop closure: TERM groups, bounded grace, KILL survivors, reap leaders, preserve private logs/data. Do not signal unrelated processes or close the user's browser. No public-internet source fetches or replies.

Seed module contract: export async seedPersonal({baseUrl,token,signal}) and seedProvider({baseUrl,signal}). Personal creates due/inbox note, article/text capture, supplied RSS subscription/items, source activity fixture through existing JSON API; return summary counts. Provider creates reserved demo@tangled.invalid account using a random password, publishes a sample post through existing form API, returns {email,password,...}; no email/external actions. Fetches bounded, abortable and redirect-safe; no secrets in errors. Root saves returned credentials in private0600 file and prints demo sign-in details. Existing reed-seed/loomed-seed with --create-local plus weft-seed run against owned knot and isolated REED_HOME/LOOMED_HOME/WEFT_HOME.

Root owns supervisor/justfile/README. Agents use own native managed worktrees, only assigned files, tests red first, commit. No concurrent Rust builds from agents. Lifecycle fixture tools are found through PATH and target_directory (fake cargo metadata); no production-only command injection flags.
