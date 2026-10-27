# Single-owner utility validation

Code reviewed and tested through f63efe2 on codex/solo-usefulness, based on master32ba17f. User authorized eight feature subagents/worktrees, validation, and local master integration. No push or deployment was requested.

## Delivered contracts

Eight reusable workflow modules share an ordinary signed KnotEngine repository and extensible collection/body schemas. One JSON dispatcher serves both Rust callers and the command client. Third-party application collections remain readable, searchable and exportable; unknown body fields survive edits. Public namespace names do not imply public visibility. Private sources require deliberate confirmation before selected-content publication.

## Independent review

All eight features reviewed independently. Four confirmed first-pass issues fixed in consolidated commit3d8e5b1: signed MST-authoritative CAR import, recoverable import/sync publication, tombstone-aware agent writes, and safe article/feed search adapters. Regression work additionally covers interrupted publication prefixes, stale materialized tips, and filesystem key collisions. Reviewer reran focused original reproductions against integration and found no remaining merge blockers. Canonical source identity hashing was additionally checked under serde_json/preserve_order and against JavaScript BLAKE3 golden vectors.

## Verified results

- `cargo test --workspace --offline`: 540 passed,0 failed,7 ignored across88 test suites.
- `cargo nextest run -p tangled-personal --offline`: 99 passed,0 skipped; final run had no leaky tests.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo test -p tangled-personal --features serde_json/preserve_order --test connections --offline`: passed.
- Real CLI smoke: all8 workflows; separate commands share signed data, article/feed body search, static selected-content export, encrypted backup verification, restore identity and unknown extensions: passed.
- `npm test --prefix site`:4 passed.
- `node --test tools/capture/capture.test.cjs`:2 passed.
- `npm run build --prefix site` plus `npm run check:docs --prefix site`: passed;86 HTML pages,3473 local links, raw spec assets and search output checked.
- JSON syntax checks:33 schema/vector documents parsed.

Logs remain in /tmp/tangled-solo-*.log for this session. User-facing commands and exact constraints are in docs/features/personal.md and its eight linked feature pages.

## Operating limits

Source/feed ingestion accepts supplied content; no new live source authorization, remote extraction, connector delivery or background fetch scheduler is claimed. Device continuity uses explicit encrypted file transfer with full owner keys; those replicas cannot be individually revoked. Notes reminders are queryable without a notification daemon. Publishing emits selected static files/public records without providing hosting or a guest-link service. Capability-gated personal records remain readable by their local/custodial operator. Recoverable snapshot publication covers import/sync/restore; ordinary writes retain the core persistence contract.

## Preservation and final integration

Original master checkout has100 pending paths, with only Cargo.lock overlapping this branch. The final merge will retain a private byte/index backup and named recovery stash, then compare all original pending files (and an independent three-way merge for Cargo.lock) after restoration. Actual master validation and preservation results are recorded after integration.
