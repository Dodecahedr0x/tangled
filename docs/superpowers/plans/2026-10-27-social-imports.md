# Social imports implementation plan

> **For agentic workers:** Execute continuously. The user explicitly waived brainstorming and further feedback gates, and requested an assumption list for review after implementation.

**Goal:** Import social archives and continuously ingest connected source activity into a private, portable Tangled archive on hosted and self-hosted accounts.
**Architecture:** Shared Rust parsing/model/storage crate; authenticated owner archive HTTP/UI in the knot and provider; source-specific connector helpers ingest through connection-scoped tokens. Preserve original uploads and normalized data as encrypted signed repository records/blobs.
**Tech stack:** Rust, axum, SQLite, existing Tangled HPKE and signatures; source-native connector runtimes where required.
**Spec:** `docs/superpowers/specs/2026-10-27-social-imports-design.md`.

## Global constraints

- Preserve the preexisting dirty checkout. Work on `codex/social-imports`; do not publish, deploy, or commit unrelated changes.
- Private by default; foreign actors are metadata, never fabricated Tangled identities. Hosted custody remains explicit.
- Retain original uploads, attachment bytes, unrecognized fields, source times, and distinct repeated messages.
- Write no source credentials into repositories. Scope ingest tokens to owner + source connection, and invalidate them on disconnect.
- No fake Connect success. Adapter dependencies, pairing/authorization, historical limitations, and live validation gaps must be stated accurately.
- Use meaningful failing tests before implementation. Test isolation, malformed archives, duplicate/revision handling, replay, source access failure, and exports.

## Shared interfaces and ownership

`crates/tangled-import/src/model.rs` (parser task owns): serde types `ImportedObject { id: String, kind: String, conversation: Option<String>, sender: Option<String>, timestamp: Option<String>, text: Option<String>, attachments: Vec<String>, data: serde_json::Value }`; `ImportedFile { name: String, media_type: String, data_base64: String }`; `ImportBatch { source: String, account: String, objects: Vec<ImportedObject>, files: Vec<ImportedFile>, warnings: Vec<String> }`. All optional/vector/data fields use serde defaults. Public normalized source names: `telegram`, `twitter`, `whatsapp`. JSON live batches use these exact fields. IDs are source-native where available and otherwise documented stable fingerprints with repeat-occurrence indices.

`crates/tangled-import/src/parsers.rs` (parser task owns): `pub fn parse(source: &str, account: &str, filename: &str, bytes: &[u8]) -> Result<ImportBatch, String>`. Account argument identifies the source account being imported; prefer stable native identity from the archive when present and report mismatches rather than silently mixing accounts.

`crates/tangled-import/src/lib.rs`, `Cargo.toml`, `store.rs` (root owns): export model/parser APIs and encrypted archive store. Root owns all workspace manifests, provider/knot integration, UI, migration, and final docs.

`connectors/` (live-adapter task owns): executable source-specific authenticated ingestion helpers plus tests and README. Inputs: destination base URL and connection token, source account/configuration. Output: HTTP `POST /api/imports/connections/{connection_id}/ingest`, `Authorization: Bearer <token>`, JSON `ImportBatch`. Destination source/account must match the registered connection. A successful response means the batch has been durably imported. Retry safe: stable native IDs and original timestamps. Helpers must persist cursors only after success. Source login stays in the helper; the host never receives source credentials.

## Review focus

1. Archive/live overlap and repeated identical messages must preserve distinct native objects.
2. Public sync/blob/subscriptions and another account must never reveal private content.
3. Disconnect and destination account migration must fence stale connector writes.
4. Archive extraction must reject unsafe paths, symlinks, bombs, and executable JavaScript wrappers.
5. Migration must preserve encrypted raw data and archive readability, not only public posts.

### Task 1: Source archive parsing

- [ ] Add failing real-format parser tests for Telegram JSON/ZIP, X wrapped archive data/ZIP, WhatsApp multiline chat text/ZIP, media, duplicate occurrences, unsafe archives, malformed JSON, and unknown data retention.
- [ ] Implement the shared models and parsers at the exact interfaces above; bounded safe in-memory ZIP reading without extraction, source-native identity and raw data preservation.
- [ ] Verify `cargo test -p tangled-import --test parsers`; report format/coverage limits and files changed. Do not edit root-owned files or commit shared work.

### Task 2: Independent live connector helpers

- [ ] Add failing tests for destination ingestion authorization/retry/checkpoints, mapping of source messages/media, and bounded source failures.
- [ ] Implement actual Telegram user login/backfill/updates, X user authorization/backfill/polling, WhatsApp linked-device login/history/updates using maintained source libraries. Dependencies and credentials are setup requirements; never claim mock coverage is live validation.
- [ ] Keep helpers read-only; retain source IDs and bytes; protect locally persisted credentials and avoid exposing ingest tokens in logs/arguments where practical. Include installation and unattended deployment instructions.
- [ ] Run helper unit/protocol tests and syntax/build checks; document source-account testing still requiring user credentials.

### Task 3: Shared encrypted store and repository projection

- [ ] Write failing owner-isolation, reimport/edit ordering, encrypted-at-rest, original-download, persistent restart, connection-token revoke, and deletion replay tests.
- [ ] Implement store and signed sealed repository projection; chunk original uploads and attachments, keep stable object keys, persist durable receipts, and support exporting/importing private archive state without source credentials.
- [ ] Verify new crate tests and clippy; record assumptions and deviations in the implementation log.

### Task 4: Host API and archive experience

- [ ] Add failing provider and knot HTTP tests covering authenticated upload/list/download, anonymous/other-owner denial, source-scoped connection ingestion and disconnect, and public sync exclusion.
- [ ] Mount shared authenticated import routes and an accessible archive page in both hosts. Provider uses existing login; self-hosted uses identity session or local authenticated helper. Add home/settings entry points.
- [ ] Implement resumable job/receipt reporting, source availability, coverage warnings, original download, and read-only normalized browsing including media/conversations and source filters.
- [ ] Verify HTTP tests and inspect rendered UI at desktop/mobile sizes.

### Task 5: Migration, docs, and final review

- [ ] Add a failing provider move test with private records/blobs and archive key material; fix authenticated handoff and destination persistence before retiring source custody.
- [ ] Document assumptions, supported formats, connector setup/hosting, limits and validation evidence. Update product/readme claims accurately.
- [ ] Run formatting, clippy, targeted tests, and workspace tests. Obtain a fresh code review, fix material findings, and report exact remaining external setup/live validation gaps.

## Implementation log

- User instruction: keep building without feedback; no further design/plan approval gate.
- Ruling: retain current checkout on a new branch rather than create a clean worktree, because required provider/application functionality is currently uncommitted.
- Ruling: independent archive parsers and external live connector helpers can be implemented in parallel against the fixed JSON contract; root integrates and tests the host/store.
- Ruling: source credentials remain in connector helpers, not the Tangled host. This supports self-hosted or provider-operated unattended helpers and user-run helpers without inventing generic OAuth for Telegram/WhatsApp. Hosted one-click pairing is not claimed unless implemented.
- Implemented: source parsers and retained media; encrypted owner archive/database/repository projection; native-ID/revision reconciliation; immutable file aliases; bounded HTTP uploads, private browsing/search/original downloads and file listing; source-scoped helper ingestion and token revocation; real Telegram/X/WhatsApp helper authentication/runtime paths; provider migration with destination acknowledgement; independent passphrase recovery and offline CLI; Reed/Loomed account-archive links.
- Review fixes: enforce core raw-block authorization for private records/blobs; normalize X/Telegram archive/live identities; compare source revision instants; retain ambiguous WhatsApp overlap candidates; preserve repeated media filenames immutably; link X video/GIF variants; reject conflicting destination archive keys; recover provider-independent account/archive keys and all retained file bytes.
- Ruling: use a centralized account archive and helper-operated source login for this implementation. Dedicated in-app imported conversation rendering and browser-hosted pairing remain future work, not advertised capabilities.
- Ruling: retain bounded synchronous uploads and final durable receipts. The proposed resumable host job queue, pause/cancel, host budget scheduling, and full source/batch erasure are not implemented; the current guide states these gaps explicitly. Live helper cursors/spools are durable and independently restartable.
- Ruling: retain owner-only HPKE application archives rather than introduce a native messaging circle for foreign participants; no claim of new circle/epoch interoperability or independently signed foreign authorship.
- External validation: real source accounts and their API entitlements were unavailable; local protocol/library tests are not treated as live-account validation. No production deployment or API purchase was made.
- Assumptions and operating guide: `docs/features/social-imports.md`; source setup and category limits: `connectors/README.md`. Implementation remains uncommitted on `codex/social-imports` to avoid committing the checkout's preexisting provider/application work.
