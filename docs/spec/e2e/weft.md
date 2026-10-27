# Weft application-core cluster acceptance

Boundary: production `weft-core::Explorer` reads actual HTTP stats from three
persistent subprocess knots. Extra isolated knots host unreachable-peer and
loopback Solana JSON-RPC handle fixtures. No external DNS, chain or account
service is required. Contract sources: `apps/weft/PRODUCT.md`,
`apps/weft/core` and `tangled.node.stats`.

| ID | Observable acceptance | Test/scenario |
| --- | --- | --- |
| WEFT-counts | Known public workloads increase per-node record/identity counts exactly; assembled record/identity totals equal the sum of the three HTTP responses. | Cluster: `apps::weft` |
| WEFT-topology | All three distinct knots are reachable and home advertises peer edges to the other two knots. | Cluster: `apps::weft` |
| WEFT-lookup | Knot URL returns the corresponding knot; a hosted public DID returns its matching record count; empty lookup fails. | Cluster: `apps::weft` |
| WEFT-handles | A loopback SNS RPC fixture proves `cluster.sol` forward ownership; one-sided ownership fails, then a signed `sys.id` reverse claim enables correct DID/handle resolution. The fixture observes actual `getAccountInfo` requests. | Cluster: `apps::weft` |
| WEFT-unreachable | An advertised unavailable loopback peer remains in topology with `reachable=false` and no fabricated stats. | Cluster: `apps::weft` |
| WEFT-private-exclusion | Publishing sealed chat after public circle metadata leaves public records/circles/identities/blobs counts unchanged; stats contain no private text. | Cluster: `apps::weft` |
| WEFT-read-only | Stats, topology and lookup reads leave all original knots' persistent aggregate counters unchanged. | Cluster: `apps::weft` |

Counts describe public live record tips and referenced blobs, not every stored
historical object. Public circle metadata remains visible by design. The suite
does not interpret peer advertisement as automatic record replication.

Supplemental existing coverage, separate from cluster acceptance:
`apps/weft/core/tests/core.rs` exercises mock-engine totals/topology/lookup and
constructor errors. `crates/tangled/src/handle.rs` tests injected DNS/SNS resolver
proofs and account decoding; `crates/tangled/tests/sns_rpc.rs` owns independent
resolver RPC tests. None substitute for the real HTTP checks above.
