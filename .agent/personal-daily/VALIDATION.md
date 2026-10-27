# Personal daily workflows — 2026-10-27

Implemented the continuation in three isolated managed worktrees: Today, Notes safety/Library-Capture-Notes record links, and Feed/Connections record links. Source integration branch codex/personal-daily-workflows started at b104429; source merged to master in 4b129f9.

## Result

Today opens by default with due reminders, self inbox, unread saved items and unread feed articles. Each section has independent loading/retry, up to ten visible records, direct links and reversible quick actions. Undo lasts while Today remains open. Main data stays in existing signed collections.

Notes guard unsaved main fields, append text, selected attachments and pending requests across same-pane replacement, route changes and browser unload. Same-note updates retain unrelated native controls without persisting drafts in browser storage. Successful actions clear their submitted input; errors retain it. Typing during save and stale/detached completions are covered.

Copy link and #view?item=percent-encoded-Tangled-URI routes open exact local records, including unfamiliar collections through Library and items outside current filters. Record selection updates the address without remounting. Links omit owner credentials and confer no access. Discovery advertises the link template. Cold authentication retains the destination; invalid/foreign/wrong-collection targets do not write data.

## Validation

Against actual merged master including the restored pre-existing pending changes:

- 103 tangled-personal Rust tests passed, zero failed or ignored.
- Strict Clippy for tangled-personal and all targets passed; full-workspace formatting and whitespace checks passed.
- 83 Personal interface tests and syntax checks passed.
- Both Chromium scripts passed with zero JavaScript errors: all eight prior workflows plus Today actions/Undo, partial section failure/retry, dirty cancel/save/failure/discard/close, cancelled stale reauthentication, copy/reload/peer/cold-auth links, current-record address synchronization, a feed target outside unread filters, and third-party unknown-field preservation.
- Documentation build/link validation: 89 HTML pages, 3,615 local links, specification assets and search output; all four documentation tests passed.

The final integration branch also exercised all nine mobile views for overflow and selected-note focus at 390px, and captured settled 1440px/390px Today screenshots. Independent root review reproduced two timing issues; both were corrected and verified with tests executing actual app.js. A fresh visual review requested compact Today spacing and keyboard continuity; the verdict pass scored both resolved, disposition ship. The design documenter confirmed the extension preserves the incumbent app world. One design detector pass returned no findings.

All 100 original pending paths retain exact hashes and modes and an empty staging area. There was no merge overlap, no stash was required, and none of that work was committed. A private exact backup remains outside the repository. Feature worktrees were submitted to the native archival tool; the integration checkout is retained and synchronized with committed master.

No deployment, external feed fetch, reply delivery, background reminder notification or new private data silo was added. These remain documented boundaries. The full workspace was validated in the preceding interface milestone; this pass reran the affected Rust crate rather than claiming another full-workspace test run.
