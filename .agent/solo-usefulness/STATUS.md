# Solo utility execution

Base master: 32ba17f. Main checkout has substantial pre-existing dirty work; do not commit it as part of this feature. User explicitly authorized eight separate feature worktrees/subagents, validation and merge to master.

Integration worktree: /Users/dode/.codex/worktrees/solo-integration/tangled
Integration branch: codex/solo-usefulness
Contracts: contracts.md. Plan: docs/superpowers/plans/2026-10-27-single-owner-utility.md.

- Shared signed Store: e84463f; unlinked blob/get correction 836b63e. Four Store tests pass; generic protocol reader test added.
- CLI / reusable dispatcher: 1ac6b04; two command tests pass; first wave package tests all pass.
- 1 memory: integrated 92f5f53 (source agent 1e5dbba); 9 tests + clippy pass.
- 2 capture: integrated d8be5f0,7893618 (source 6c3c0cc,9730805); 7 tests + 2 helper tests pass.
- 3 notes: integrated 3863936,e87a5bb (source bc4e758,9e147be); 7 tests + clippy pass with Store blob fix.
- 4 feeds: active agent /root/feeds in solo-feeds, base e84463f; quick-xml/url deps approved.
- 5 agents: active /root/agents in solo-agents; owns store/delegation.rs; parent must wire mod delegation.
- 6 devices: active /root/devices in solo-devices; owns store/replica.rs; parent must wire pub(crate) mod replica; Argon2id/HPKE backup and restore.
- 7 publish: managed worktree solo-publish ready at 48cc66b; agent not yet spawned (3-slot wave limit).
- 8 connections: managed solo-connections ready at 48cc66b; agent not yet spawned.
- Independent review and combined validation: pending.
- Merge/preserve/verify master dirty work: pending.

Do not share CARGO_TARGET_DIR across concurrent worktrees: same package artifact caching produced spurious import/test failures. Root final target is /tmp/tangled-solo-integration-target; workers use worktree-local targets. Initial full workspace baseline failed one Store test due shared-target old artifacts, so it is NOT a passing baseline. Repeat once all features are integrated in dedicated target.
