# Single-owner utility implementation plan

> **For agentic workers:** Use subagent-driven development with separate feature worktrees, as explicitly requested by the user. Continue through validation and merge to master.

**Goal:** Make one Tangled owner useful without other network users.

**Architecture:** Eight independently implemented modules use the existing signed repository engine through a shared adapter. Records, schemas, generic protocol exports and reusable APIs support independently built apps; no frontend owns the data. A common JSON CLI supplies a working entry point for every module.

**Tech stack:** Rust, existing Tangled identity/capability/HPKE/CAR implementations, cached quick-xml and URL parsing; Argon2id for portable encrypted recovery.

**Spec:** `.agent/solo-usefulness/brief.md` and `contracts.md`, derived from the eight proposals and the user's instruction to implement, validate and merge them.

## Global constraints

- Preserve all existing uncommitted master work; implement on separate worktrees.
- Signed ordinary records and blobs, public extensible schema contracts, private defaults.
- Unknown collections remain retainable, unknown body fields survive updates.
- Explicit source writes/publication only; do not fabricate users or signatures.
- Capability gating does not hide plaintext from the custodial/local host.
- No deployment, external account authorization or API purchase.

## Review focus

- Unknown-client extension fields and stable URI/exact CID evidence survive edits.
- Invalid inputs, UTF-8, oversized supplied content and malformed XML fail safely.
- Private bytes do not enter public pages, feeds, capture or logs accidentally.
- Capability audience, scope, expiry and revocation are checked cryptographically.
- Backup tampering, wrong passphrases, stale heads and wrong identities fail without destructive mutation.

## Tasks

For each task: write behavior tests, observe failure, implement, run focused package tests and lint, commit only its paths. Parent reviews and integrates feature commits, wires shared boundaries, and runs the full combined workspace suite.

1. Searchable personal memory: memory.rs, tests/memory.rs, generic saved-search/collection/annotation schemas, feature docs. Acceptance: cargo test -p tangled-personal --test memory.
2. Capture/read-later: capture.rs, tests/capture.rs, capture schema/docs and browser JSON capture helper. Acceptance: cargo test -p tangled-personal --test capture; node --test tools/capture/capture.test.cjs.
3. Notes/self inbox: notes.rs, tests/notes.rs, note schema/docs. Acceptance: cargo test -p tangled-personal --test notes.
4. RSS/Atom reading: feeds.rs, tests/feeds.rs, subscription/item schemas/docs. Acceptance: cargo test -p tangled-personal --test feeds.
5. Agent workspace: agents.rs, store/delegation.rs, tests/agents.rs, activity schema/docs. Acceptance: cargo test -p tangled-personal --test agents; expired/wrong audience/wrong scope/revoked grants rejected.
6. Device continuity: devices.rs, store/replica.rs, tests/devices.rs, backup/device contracts/docs. Acceptance: cargo test -p tangled-personal --test devices; encrypted restore retains identity, records and attachments.
7. Independent publishing/sharing: publish.rs, tests/publish.rs, publication schemas/docs. Acceptance: cargo test -p tangled-personal --test publish; selected material readable without account, unselected private bytes absent.
8. Connected-source organization: connections.rs, tests/connections.rs, source-overlay schemas/docs. Acceptance: cargo test -p tangled-personal --test connections; provenance retained, source activity not silently republished or sent.
9. Parent integration: store.rs, lib.rs, main.rs, tests/store.rs, tests/cli.rs, cross-client tests and umbrella docs. Acceptance: cargo test -p tangled-personal; cargo clippy -p tangled-personal --all-targets -- -D warnings; cargo test --workspace; formatting/diff checks. Review combined feature code independently, address findings, merge to master and restore/verify pre-existing work.

## Execution notes

The user explicitly selected separate subagents/worktrees and authorized merging; no additional execution-method approval is required. First versions use supplied file/content transfers where external live service or mobile integration would require new accounts or another product. Those limitations must remain visible in docs and final report.
