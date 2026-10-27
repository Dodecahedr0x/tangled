# Selected research integration

The selected changes are merged into master. Integration started at 565edc1 and code integration finished at f39a77b; the historical snapshot 6baf075 was not merged.

## Included scope

- **08:** seeded delivery-fault scheduler and regressions, preserving master's newer startup-log lifecycle.
- **07:** trusted offline capture/verify/restore/scrub/repair, persistent erased-CID admission fence, CLI/default binary selection and [recovery runbook](../guides/recovery.md).
- **01, fixes only:** existing authorized exports enforce exact-record, repository-DID and circle scope before returning records/blobs. Invalid foreign-DID requests do not consume a one-use capability. No inventory endpoint or client was added.
- **06, fixes only:** iterative causal closure, frontier and RGA traversal. The regression exercises 100, 000 atom apply/snapshot and 10, 000 record apply/frontier. No new default budgets or error variants were added.
- **03, hardened:** bounded persisted peers and guarded discovery. Learned loopback destinations require the explicit default-off --allow-learned-loopback development option. A DNS answer set must be entirely public or, with that explicit option, entirely loopback. Mixed answers, LAN/metadata destinations, redirects and proxies remain denied for learned discovery. Configured exact seeds are the operator trust boundary.

Experiments 02, 04 and05 remain experimental on their original branches. The inventory extension from 01 and default-budget policy from 06 also remain unmerged. Original provenance: 2f7e086,579342d,e3044c4,85a28e0 and hardened peer worker 4408fcdf. Only selected feature deltas were ported; the old baseline snapshot and research log corpora were excluded.

## Verification

| Tree/check | Result |
| --- | --- |
| Original master workspace | 401 passed, 0 failed, 6 ignored |
| Selected integration workspace | 441 passed, 0 failed, 7 ignored |
| Merged master with original pending work restored | 514 passed, 0 failed, 7 ignored |
| Real-process selected integration | Reed, Loomed, Weft, provider, protocol passed |
| Selected workspace clippy, all targets, warnings denied | passed |
| Selected workspace formatting | passed |
| WASM library check | passed without warnings |

The additional ignored worker is launched explicitly by the deep-stack subprocess regression. Before extraction, exact-record and foreign-DID scope regressions failed, and the deep-page child reproduced stack overflow. After extraction they pass. Independent read-only combined review found no blocking issue. The task artifact bundle contains exact commands, raw logs, process reports and the post-merge preservation audit.

A real-process run exposed an existing provider-only harness indexing an intentionally empty knot list. The user's pending work already contained the identical one-line source-URL correction; it was included to complete the process suite. One preexisting circle-export assertion was formatted for the format gate. Other unrelated pending work was restored rather than committed.

## Preservation and operating limits

Pending main-checkout changes were saved privately, stashed, and reapplied with the index after the fast-forward merge. There were no conflicts. All 102 original paths were verified: paths outside the selected changes remained byte-identical, and overlapping paths matched an independent three-way merge of the old base, selected commit and original bytes. The recovery stash is retained and referenced in the task report.

Recovery is an unencrypted, trusted offline administrator tool. It does not recover keys/importer stores, authenticate publishers, prove freshness or silently overwrite valid newer metadata. Routine peer-state advancement also causes older-snapshot repair to refuse; repair requires the matching stopped snapshot. New explicit discovery permission applies only to learned loopback peers, not arbitrary private-network URLs or unrelated AP/content fetch paths.
