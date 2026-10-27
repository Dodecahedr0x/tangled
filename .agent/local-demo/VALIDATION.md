# Managed local demo validation — 2026-10-28

`just demo` (or `node scripts/local-demo.mjs`) prepares and starts Knot, Personal, provider, docs and the three native apps, seeds their isolated stores/profiles, opens the browser interfaces, and stays in the terminal until Enter/EOF, INT/TERM/HUP, app closure or failure. Each run has a private fresh directory with retained logs/data. Browser openers remain outside owned process cleanup so an existing browser is never killed.

Implemented by root supervisor plus isolated seeding and lifecycle-test worktrees. Independent review found browser-opener ownership and cached native URL issues; both were fixed. Native Settings now honor runtime TANGLED_PERSONAL_URL; wasm retains compile-time configuration. A failing opener is reported asynchronously with a saved-link fallback. Source merged to master in 1df8fca.

Actual merged master checks, including all preserved pre-existing work:

- 40 Node tests passed: options, authenticated bounded local seeding, real child/grandchild process-group cleanup, Enter/EOF/INT/TERM/HUP, startup/seed/service failures, native app exit, occupied listener preservation, missing executables, long-lived browser openers, and failed openers.
- 70 Rust app tests passed; strict default-feature/all-target Clippy, formatting and whitespace checks passed.
- Built the real backend/seed binaries and all three desktop binaries. The normal prepare path with all desktop apps ran successfully on macOS, using an alternate port offset and fresh temporary state.
- Real samples verified through Personal HTTP API: two notes, two feed articles and one source item, alongside two captures. Provider signup/sample post and documentation startup succeeded. Shutdown left no demo HTTP listeners. Native process groups exited successfully.
- Exact `just demo` terminal entry was exercised with Ctrl-C; the wrapper returned conventional status 130, and the supervisor log confirmed complete cleanup.
- Native browser opening is covered with real process fixtures; actual test runs used --no-open to avoid leaving browser tabs. Persistent opener processes are intentionally excluded from managed shutdown; only fixture teardown kills its own fake browser processes.

Real-stack testing exposed transient EPERM on zero-signal process-group probes during exit. The supervisor now conservatively waits for those groups to disappear, with TERM/KILL and reap deadlines. Successful short-lived build/seeder groups are retired promptly. No process-name or port-based killing is used.

All 100 original pending paths and the original empty staging area were retained. README and two Settings overlaps match independently constructed combinations; other hashes/modes match the private exact backup. No unrelated pending work was committed. Separate feature worktrees were submitted for native archival; the integration worktree remains synchronized with committed master.

The launcher is macOS/Linux-only and requires Node 22+, Rust and native desktop prerequisites. --no-desktop supports headless hosts. Provider's existing all-interface listener is unchanged and documented. Demo data/logs are retained; browser tabs may stay open after their services stop. No external source account, live feed fetch or source reply is performed.
