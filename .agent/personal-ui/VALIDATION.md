# Personal app validation — 2026-10-27

The Personal browser app and interoperable local API were merged to master in d37b97d. The following checks passed against the actual main checkout, including all restored pending user changes:

- `cargo test --workspace --offline`: 617 passed, 7 existing ignored, 0 failed.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- Personal `npm run check` and `npm test`: syntax passed; 34 interface tests passed.
- Personal `npm run test:browser` using the binary built from actual master: all eight workflows passed with no JavaScript errors. Temporary test libraries are removed after each run.
- Documentation build, link validation and tests: 89 generated HTML pages, 3,614 local links, specification assets and search output checked; 4 tests passed.

Chromium exercised persistent note/draft creation with recoverable failures, exact attachment downloads, capture/read/star, cross-collection search, RSS import and inert readable HTML, imported source organization, approval-bound reply export, scoped certificate download and revocation, selected-content HTML/publication creation, encrypted recovery export/verify/restore and third-party extension preservation. It also covered launch fragments on already-open tabs, a launch fragment arriving during peer discovery, stale sessions, same-origin tab connection, inherited hash names and the keyboard skip link. The mobile pass checked overflow across all eight views at 390px and confirmed explicit selection focuses and scrolls the loaded editor into view.

Independent backend/client authentication review completed; its reconnect findings were fixed and covered by browser regressions. The design detector returned no findings. A fresh visual review requested mobile detail reveal after explicit selection; the follow-up scored that material fix resolved with disposition `ship`, with no capture regressions. The design documenter compared shipped styles with the incumbent app world. The Personal brief now records the actual 19px reader and 1,320px main maximum.

Preservation: all 100 pending paths and the original empty staging area were restored and verified. Unaffected files match their exact backed-up hashes and modes. Cargo.lock and Reed/Loomed settings match independently reviewed combinations; the user's native ConnectedApps routes remain intact. A private exact backup and the preservation stash remain available outside the repository. No pending user changes were committed.

No deployment, source account connection, source reply delivery, background feed fetching or background reminder scheduling is implied. Those remain explicit boundaries documented in the app and API guide.

Four managed feature worktrees were used. Native archival was attempted after integration; the desktop app retained them because other active tasks still reference them. No forced removal was performed. The retained integration worktree is synchronized with committed master.
