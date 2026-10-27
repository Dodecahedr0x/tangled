# Selected Research Integration Implementation Plan

> For agentic workers: use superpowers:executing-plans for parent integration and a separate isolated peer-discovery worker. Track concrete verification and preserve the user's main-checkout work.

**Goal:** Integrate approved research08/07, only authorization-scope fixes from01 and iterative stack-safety traversal from06, and explicitly hardened03 into master, leaving02/04/05 experimental.

**Architecture:** Start from current master, not the historical snapshot6baf075. Apply only reviewed feature deltas and adapt to improvements already present on master. Perform combined verification before touching the dirty main checkout; preserve and reapply all original uncommitted content during the final merge.

**Tech stack:** Rust workspace, Cargo tests/clippy, Python/Node connector checks where affected by restoring the user's work.

**Spec:** User's approved recommendations in this chat; original experiment reports under the saved tangled-research-results bundle.

## Global Constraints

- Do not merge the common baseline snapshot or unselected02/04/05 code.
- Preserve current master startup-log retention and circle filtering, which are newer than the research baseline.
- Keep01 inventory endpoint/client and06 default budgets out of the merged change.
- Learned localhost destinations require explicit development opt-in, default off; exact configured seeds retain operator trust.
- Recover only trusted offline snapshots; preserve erasure fences and refusal of valid newer mutable metadata.
- Do not commit unrelated main-checkout work. Do not force-reset or drop any stash.

## Review Focus

- Capability scope is checked for exact record, repository DID, circle and private blob exports on legacy routes.
- Deep causal/RGA traversal succeeds without changing semantic results or imposing new default limits.
- Existing startup failure evidence remains retained when adding the seeded scheduler.
- Offline recovery preserves newer master/import erasure behavior and default binary selection.
- Explicit peer-development mode cannot authorize LAN/metadata destinations or be inferred from the advertised URL.

## Tasks

- [x] Establish current master baseline and record failures; identify preexisting user changes.
- [x] Integrate08 scheduler/tests while retaining master's existing startup-evidence implementation; add missing bounded-log tests.
- [x] Integrate07 complete offline recovery delta including its recovered prerequisite tests/plan; adapt to current engine and preserve CLI default.
- [x] Extract01 legacy export authorization fixes and focused RED/GREEN regressions only.
- [x] Extract06 iterative causal/RGA traversal and focused RED/GREEN deep-chain regression only.
- [x] Integrate worker's03 hardened peer policy and its explicit opt-in regressions/docs.
- [x] Run combined workspace tests, focused negative tests, changed-file formatting, clippy and applicable WASM checks; fix any integration regressions.
- [x] Independently review the combined diff; keep unselected experiment modules/endpoints absent.
- [ ] Preserve main dirty state, merge selected commits into master, restore original work with conflict-aware comparison, rerun checks on the resulting combined working tree.
- [ ] Save integration report with exact commands/results, merge commits and preservation evidence.

## Evidence and integration decisions

- Current master baseline: cargo test --workspace --no-fail-fast ->401 passed,0 failed,6 ignored.
- RED scope tests reproduced exact-record widening and foreign-DID acceptance; the matching one-use capability must remain unspent after rejection. GREEN3/3.
- RED deep-page subprocess aborted on stack overflow. GREEN includes100,000 atom apply/snapshot and10,000-record apply/frontier, without importing06 budgets or Error variants.
- Initial combined acceptance:33 passed,0 failed,1 subprocess-only ignored worker; full selected workspace after03:441 passed,0 failed,7 ignored.
- Master's startup log lifecycle and circle filtering are retained;08 adds scheduler and additional tests rather than replacing newer implementations.
- Recovery repair intentionally refuses differing valid peer metadata as well as authority state; matching-snapshot requirement is explicit in the runbook.
- Full workspace formatting found one pre-existing circle-export assertion; formatting only that assertion is included so CI format check passes.
- Source review will precede final main-checkout merge. Original dirty main paths remained byte-identical to preservation snapshot during isolated integration.

Ruling: the real provider-only cluster reproduces an existing empty-node-vector panic in bound_handoff. The user's pending changes already contain the identical one-line source-URL correction. Include that minimal harness correction (no unrelated pending changes) so actual combined process tests complete; preservation audit will record the already-included change. Native tests of source permissions still pass; conditional builder mutability removes the WebAssembly warning without changing directory policy.

Final isolated checks:441 workspace tests passed,0 failed,7 ignored. Real process cluster with3 operations/256-byte payloads passed Reed,Loomed,Weft,provider,protocol. Full workspace clippy/format passed; WASM library check passed without warnings. Independent review found no blocker; matching-snapshot peer-state limitation is documented.
