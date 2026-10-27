# Personal browser app

The user approved one Personal app and explicitly requested implementation with subagents in separate worktrees, validation, then local merge to master. The app exposes all eight existing workflows through ordinary forms, readers, lists, file selection and downloads. No JSON request editor or terminal feature flow is an acceptable substitute.

Use the existing signed personal Store/execute modules as the sole data source. Expose a documented versioned HTTP adapter usable by independently built clients; preserve public record schemas, unknown fields, stable URIs and exact-CID evidence. A browser app is one client, not a new database or exclusive network.

The service binds loopback and owns the Store lock, uses per-run bootstrap authentication in a URL fragment, rejects hostile Hosts/Origins, bounds requests, and moves blocking storage/KDF work off the async runtime. Public static assets, discovery and schemas contain no personal data or session credentials. Browser backup export/import bridges existing encrypted recovery without asking users to type server paths. Restore creates a separate fresh library and switches only after verification.

The interface extends the current white/black ruled-list app design, with sparse red active/action/focus accents and a generous text reader. Desktop sidebar becomes horizontal word navigation on phones. Common UI helpers render untrusted content as text, with safe generated publication preview. Loading, empty, error, populated, destructive/public confirmation and success states are required.

Workstreams: shared shell/helpers/styles (parent), HTTP service/launch, library+capture+notes, feeds+connections, access+recovery+publishing. Feature owners have disjoint module paths; CONTRACT.md pins exports, HTTP requests and response handling. Existing apps link to the new app using an explicit launch URL/configuration. No claimed live feed fetch/source delivery, new mobile binary, background sync, hosting or deployment is part of this interface request.

Validation requires real HTTP auth/storage and third-party-client tests, browser walkthroughs of all8 workflows with real persisted state, safe file roundtrips, desktop/mobile screenshots and overflow/keyboard checks, independent code/design review, relevant Rust/JS/docs checks, preservation of original master pending work, and verification of the merged result.
