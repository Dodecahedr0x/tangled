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

- [ ] Establish current master baseline and record failures; identify preexisting user changes.
- [ ] Integrate08 scheduler/tests while retaining master's existing startup-evidence implementation; add missing bounded-log tests.
- [ ] Integrate07 complete offline recovery delta including its recovered prerequisite tests/plan; adapt to current engine and preserve CLI default.
- [ ] Extract01 legacy export authorization fixes and focused RED/GREEN regressions only.
- [ ] Extract06 iterative causal/RGA traversal and focused RED/GREEN deep-chain regression only.
- [ ] Integrate worker's03 hardened peer policy and its explicit opt-in regressions/docs.
- [ ] Run combined workspace tests, focused negative tests, changed-file formatting, clippy and applicable WASM checks; fix any integration regressions.
- [ ] Independently review the combined diff; keep unselected experiment modules/endpoints absent.
- [ ] Preserve main dirty state, merge selected commits into master, restore original work with conflict-aware comparison, rerun checks on the resulting combined working tree.
- [ ] Save integration report with exact commands/results, merge commits and preservation evidence.
