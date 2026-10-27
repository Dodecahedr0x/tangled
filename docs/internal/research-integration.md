# Selected research integration

Integrated the approved scope onto master 565edc1 rather than merging the historical snapshot 6baf075:

- 08: seeded protocol delivery scheduler and regressions, retaining master's existing startup-log lifecycle.
- 07: trusted offline snapshot/verify/restore/scrub/repair, persistent erased-CID admission fence, CLI/default binary and runbook.
- 01 extraction: exact-record, repository DID and circle export selection on the existing authorized route; no inventory endpoint/client.
- 06 extraction: iterative causal closure/RGA traversal, with 100, 000-atom and 10, 000-record regressions; no new default budgets or error variants.
- 03: bounded persisted peers, guarded discovery, explicit default-off learned-loopback development switch and all-answer DNS restrictions.

Experiments 02, 04 and05 remain unmerged. Source provenance: 2f7e086,579342d,e3044c4,85a28e0 and hardened 03 worker 4408fcdfa03e36d1b289f16b29dc6612362827be. Only selected changes were ported; original snapshot commits and research log corpora were excluded.

Current master baseline: 401 passed, 0 failed, 6 ignored. Selected combined workspace: 441 passed, 0 failed, 7 ignored. The additional ignored worker is launched explicitly by the deep-stack subprocess regression. The actual app/provider/protocol cluster passes all five surfaces with 3 operations and 256-byte payloads. Full workspace clippy and formatting, plus the WASM library check, pass. Independent read-only review found no blocking issue.

A process test exposed an existing provider-only harness indexing an intentionally empty knot list. The user's pending changes already contained the identical one-line source-URL correction; it is included so actual acceptance completes. No other unrelated pending work is committed. One preexisting circle-assertion formatting issue is normalized for the format gate. Main dirty files were copied into a private preservation directory before integration; a recoverable Git stash will be retained during final merge/reapplication. The final integration report records post-merge verification and preservation details.

Recovery remains a trusted, unencrypted offline administrator tool. It does not recover keys or importer stores, authenticate snapshot provenance, prove freshness, or permit silent rollback of valid newer metadata. Routine peer-state updates also cause repair from older snapshots to refuse. Learned loopback discovery requires --allow-learned-loopback; configured exact seeds remain an explicit operator trust boundary.
