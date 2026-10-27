# Personal app implementation plan

> Execute with separate subagents/worktrees as selected by the user. Continue through validation and local master merge.

**Goal:** Expose the eight interoperable personal workflows in a usable browser app.
**Spec:** ../specs/2026-10-27-personal-app-design.md and .agent/personal-ui/CONTRACT.md.
**Architecture:** One local server calls existing Store/execute; browser modules and independent clients share versioned JSON APIs and collection profiles. Static assets are embedded in the executable.

## Constraints

Preserve original main checkout pending work. No second private database. Unknown fields/provenance retained. Explicit publication and reviewed intent export. No untrusted HTML execution, public bootstrap token, ambient-auth cross-origin access, arbitrary browser filesystem path controls, fake rows or JSON-only feature interfaces. Keep existing application design and mobile/keyboard usability.

## Tasks

- Parent: apps/personal/web/index.html,app.js,ui.js,app.css; safe DOM helpers, owner session bootstrap, all8 navigation routes, responsive states.
- Backend agent: personal web module/tests, CLI serve path, dependencies, API docs; auth/Host/Origin/limits, one Store owner, third-party API, embedded assets/font, encrypted browser backup bridge.
- Library agent: library.js and tests; search/organizers, capture+reader, notes/drafts/inbox/reminders/attachments/source conversion.
- Reading agent: reading.js and tests; feed subscriptions/XML upload/queue/state/exports, source activity file import/read/organization/contacts/revisions/approved reply intent.
- Manage agent: manage.js and tests; real scoped grants/revocation/activity, encrypted backup download/upload/verify/restore/sync, explicit selected-content preview/download/publication.
- Parent integration: existing-app entry points, docs, real browser matrix at desktop/mobile using real server data, independent review and fixes, Rust/JS/docs checks, backed-up main merge and post-merge verification.

Each agent observes failing behavior tests before implementing and commits only its owned paths. Root integrates only completed work and verifies exact interfaces. Backend alone compiles Rust during feature work to avoid concurrent same-package cache collisions; parent performs combined validation afterward.
