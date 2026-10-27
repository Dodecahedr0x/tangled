# Personal Daily Workflows Implementation Plan

> For agentic workers: use superpowers:subagent-driven-development for isolated feature tasks.

Goal: make the existing Personal app useful daily and safe to leave while keeping records interoperable.
Architecture: existing Store/execute is source of truth; browser Today aggregates existing APIs, stable URI links select existing records, and ephemeral dirty-state guards protect editors. No new runtime dependencies or data schemas.
Tech stack: existing Rust loopback service and plain browser ES modules, Node tests and Playwright.

- [x] Root: branch from master, pin shared contract, dispatch three agents in separate worktrees.
- [x] Today agent: failing tests then today.js; due/inbox/unread lists, direct open, reversible actions, partial errors and bounded results; commit.
- [x] Notes agent: failing tests then library.js; dirty snapshots for note editor, confirmation before same-pane replacement, route guard registration, initial URI/mode selection in Library/Capture/Notes and Copy link actions; commit.
- [x] Reading agent: failing tests then reading.js; initial URI selection for Feed/Connections even outside current filters, generation-safe loads, Copy link actions, safe unavailable/foreign records; commit.
- [x] Root: tested routes.js parser/formatter, Today nav, authenticated bootstrap preserves safe destination, guard across hash/nav/beforeunload, copy links without credentials, embed assets/discovery; integrate commits.
- [x] Real Chromium: Today mutations update canonical records; dirty cancel/accept/save/failure/close; record link reload and invalid/foreign URI; third-party record links and extension preservation; existing eight workflows and mobile overflow/focus.
- [x] Fresh review plus desktop/mobile visual evidence; fix material findings, run scoped Rust/UI/docs checks, preserve latest pending main exactly, merge master and validate combined tree.

## Shared client contract

ctx.selection = {uri?:string, mode?:'note'|'draft'|'inbox'} for the current route. Empty selection is ordinary mount. ctx.navigate(view,{uri,mode}) formats a safe hash. ctx.copyRecordLink(view,uri) copies a same-origin public app link without owner token, announces success and throws a friendly failure; ctx.run catches it. URI links still require normal authentication and confer no access. Foreign/unavailable records show clear errors without mutation. Generic Library handles unfamiliar local collections.

ctx.setNavigationGuard(check) registers a synchronous predicate for current editor dirty/pending state; root clears it only after an accepted route change. ctx.confirmDiscard() returns true when clean or after the native discard confirmation, false on cancel. Notes uses it before selecting another record, new-note/draft/inbox, conversion and deletion as appropriate. Register fresh predicate on every editor render; compare actual current control values to initial saved snapshot. After save failure guard remains; a successful save is clean. Do not silently discard input during attachment updates or asynchronous save refresh. Root protects beforeunload and hash navigation; no secret or draft persistence in browser storage.

Leaves import existing DOM helpers; optional ctx methods may be guarded in existing unit test mocks but add meaningful tests for new behavior. Root supplies revealSelection as before. Do not edit shared shell/ui.js/Rust/docs from leaf tasks. No external fetching, reply delivery, provider auth or background scheduling is added.
