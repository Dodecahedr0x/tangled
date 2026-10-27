# Import lifecycle implementation plan

The user requests browser-hosted pairing, resumable uploads, and full data erasure, with continuous execution and assumptions listed at completion. No brainstorming/approval gate applies.

## Contracts and file ownership

- Root: `crates/tangled-import/src/store/uploads.rs` with `impl ArchiveStore` encrypted durable upload/checkpoint APIs; root registers child modules in `store.rs`. Shared HTTP/UI integration is root-owned in `crates/tangled-knot/src/imports.rs` and `imports.html`. Root owns manifests, documentation, and final verification.
- Hosted pairing implementer: only `connectors/hosted/**`, necessary existing connector auth/worker extension hooks, dedicated tests, plus `crates/tangled-knot/src/imports/pairing.rs`. Child Rust module can use `super::{ImportState,auth,same_origin,bad,failure}`. Export `pub(super) fn router() -> Router<ImportState>`. Root mounts it before `.with_state(state)`. Do not edit root's imports.rs/UI/store/manifests. Python service uses authenticated local HTTP control, encrypted per-owner/source sessions, real source native APIs, and durable worker control. Explicitly expose availability/configuration failure.
- Erasure implementer: only `crates/tangled-import/src/store/erasure.rs`, dedicated tests, plus a precise new public `KnotEngine::erase_private_archive(owner, record_cids, blob_cids)` method and necessary engine persistence loading paths. Root registers `pub mod erasure` in store.rs. Export `ArchiveStore::erase_source(owner,id,engine,source,account)->ErasureReceipt` and `erase_all(owner,id,engine)->ErasureReceipt`. Private state fields/helpers are visible to store child modules. Source erasure removes all source/account normalized objects, all retained revisions/raw batches/media/chunks/credentials/connections/index rows while preserving other source/account data. Full archive erasure also removes archive keys and pending upload/job state. Tombstone replay/source generation fence prevents stale connector writes/restores. Never silently claim copied backups or remote source data are deleted.

## Required behavior

### Hosted pairing

Browser start/status/step/disconnect routes owner-authorized, same-origin mutation protected; actual Telegram phone/code/2FA, X OAuth2 PKCE state-bound callback, WhatsApp QR from a maintained personal-account linked-device adapter. Pairing automatically binds authenticated native source account and creates host ingest connection without making users copy tokens/run commands. Host operators configure source app credentials and helper runtimes. No source secrets in logs/browser storage/URLs; pairing state expiring, owner-scoped, encrypted at rest. Disconnect and erase must stop hosted workers and remove retained source authorization. Account migration must fence old ingestion and source-session control; credentials do not migrate.

### Resumable uploads

Authenticated create/status/chunk/complete/cancel APIs. Fixed bounded chunks with exact-offset checking, digest validation and replay handling. Encrypt staged chunks before durable storage, persist checkpoint/status across restart, enforce owner and current-custody authorization, and reject stale generation after erase/move. Complete is replay-safe and produces the existing receipt. The UI retains only nonsecret upload metadata locally, resumes the same file after reconnect/reload, shows progress/errors/cancel; wrong file metadata/content cannot be silently combined.

### Full erasure

Explicit in-product irreversible-action confirmation for source/account or entire import archive. Host retained normalized content, original uploads, live batches, historical private records, blobs, temporary uploads, keys and credentials are removed. Engine persistence and archives must not resurrect erased blocks on restart or move. Fence workers and pending imports before deletion. A failure must remain retryable and cannot report erased while a data store retains requested material. Preserve native/public user content and other accounts. Explain external backup/source copies cannot be recalled. Test DB rows and persisted engine blocks, not only empty UI lists.

## Execution steps

1. Add failing behavioral tests per component: owner isolation, real source auth boundaries, restart/resume, malformed/conflicting chunks, source-specific erasure, shared references, stale workers, backup/restore resurrection and restart persistence.
2. Implement component contracts; record interface decisions in this file and communicate them across implementers.
3. Integrate browser UI and HTTP routes, including accurate unavailable/error/pending/connected states and destructive confirmation.
4. Run targeted tests and full workspace/clippy/format plus connector tests. Inspect rendered UI if browser automation is enabled; otherwise disclose the environment limit and verify HTTP/JS.
5. Review implementation, fix material findings, update `docs/features/social-imports.md` and `docs/features/connectors.md`, list assumptions and validation boundaries. Keep unrelated dirty changes untouched, no publication/real account authorization.

## Assumptions

- Hosted source credentials stay on an explicitly configured private pairing/worker service and do not enter Tangled records or browser persistence.
- “Full erasure” means all retained copies controlled by this running host and pairing service, including historical archive blocks; it cannot erase exported backups or the source service.
- No external app credentials are available for live-account validation; unavailable services must be reported honestly.
- Continue on the current dirty checkout/branch; do not commit unrelated changes or deploy.

## Implementation record

- Implemented real source-specific browser pairing behind an authenticated loopback control service: Telegram phone/code/two-step, X OAuth2 PKCE/state callback, WhatsApp linked-device QR. Native source account identity creates the ingest connection automatically; source sessions and capabilities stay encrypted server-side. Supervised workers persist acknowledged checkpoints and are stopped/erased before local data removal or custody migration.
- Implemented persistent encrypted 1 MiB upload chunks, SHA-256/offset/replay checks, seven-day inactive expiry, per-owner unfinished-upload bounds, pause/cancel/resume controls, and idempotent completion. Parsing remains bounded to existing input limits and executes on a blocking worker. Browser persistence stores only upload ID/digest/size.
- Implemented source/account or whole-archive erasure with typed `ERASE` confirmation. Historical private records/blobs, original files, indexes, same-source staging, and hosted authorization are purged; opaque CID/key/generation fences prevent stale replay after restart/restore. Other owners/native records are preserved, and different-source upload checkpoints are rebased so they remain resumable.
- Review corrections include hosted Disconnect cleanup, serialization of pairing with erasure/migration, filesystem cleanup retry, pre-body generation fencing for legacy upload routes, canonical-account staging cleanup, failed/expired pairing credential cleanup/quota recovery, preservation of unrelated uploads, runtime `procps` dependency, and correct X category configuration.
- Bundled Python/Node runtime and supervised pairing into the provider Docker image. Private persisted control/encryption files are created only when absent; source application settings remain operator-owned. Disabling a previously used service preserves a pending cleanup channel or fails unsafe setup rather than claiming retained credentials erased.
- Documentation lives in `docs/features/social-imports.md`, `docs/features/connectors.md`, and `docs/features/hosted-pairing.md` following the concurrent repository docs reorganization.
- Validation boundary: local protocol/library/process/HTTP tests only; no real source account was authorized, no production deployment occurred, Docker image build unavailable without a running daemon, and browser automation was disabled in Work settings. Input limits and external backup/source-copy limits remain explicit.
- Final crash-retention correction: startup/disconnect/erasure sweep encrypted root index write fragments while preserving canonical records and other owners; unlink/fsync failures keep cleanup pending, including retries after a successful unlink but failed sync. Direct disconnect retains owner/session retry identity until cleanup succeeds; pre-rename and post-rename index failures restore the in-memory retry identity. All 34 hosted/supervisor regressions pass. Independent final review found no remaining material findings and reproduced successful retry of its formerly failing disconnect probe.
- Verification: final full Rust nextest run passed 401 tests (6 skipped); strict workspace clippy passed. Concurrent test-runner changes were preserved and their intermediate build issues resolved by their owner. Local authenticated HTTP checks confirmed all lifecycle controls, private script caching, and per-source pairing availability. All 23 existing Python connector tests and 11 Node connector/hosted tests passed, alongside the 34 hosted/supervisor cases. Formatting, diff whitespace, Python compilation, and JavaScript syntax checks passed.
