# Cluster end-to-end tests and benchmarks

Status: approved by the user on 2026-10-27; Rust app cores over real HTTP.

## Intent

Run a small local network, verify the documented behavior of Reed, Loomed,
Weft, the hosted provider, and Tangled, and measure the resources used to do
that work. Results must come from an actual run and identify the workload,
build, machine, and measurement boundaries.

The existing checkout has extensive uncommitted application, provider, and
import work. Preserve it. Prefer a new test-only workspace package and new
specification files, with small additive changes to recipes and CI.

## Approaches

1. **Selected: subprocess nodes and real Rust app cores over HTTP.** Exercise
   production routers, persistence, clients, replicas, and app operations.
   Separate processes allow attributable memory measurements. This builds on
   existing Rust integration tests and does not require a browser build.
2. Browser-driven apps against the same cluster. Adds coverage of onboarding,
   DOM interactions, and browser storage, but requires serving three web apps
   and separates browser overhead from the processing benchmarks. Choose this
   if UI flows are part of the requested acceptance boundary.
3. In-process routers or mock engines. Useful existing lower-level coverage,
   but insufficient alone for this request because process memory cannot be
   attributed and transport/process lifecycle failures can be missed.

The user selected Rust app cores over HTTP on 2026-10-27. The rest of this
document describes that approved approach.

## Feature specifications

Create five acceptance documents under `docs/spec/e2e/`: `reed.md`, `loomed.md`,
`weft.md`, `provider.md`, and `protocol.md`. Each feature has a stable ID,
source contract, observable acceptance criteria, scenario/test identifier,
and coverage level. Distinguish cluster coverage from existing unit/vector
coverage. An unchecked required feature must fail the coverage check; do not
claim all features pass by narrowing the manifest after a failure.

| Surface | Contract and scenario families |
| --- | --- |
| Reed | Identity generate/import; text, replies, quotes, edits, deletion, image/video blobs; follow/unfollow; profile, like/unlike, boost/unboost, mute/unmute, block/unblock; cross-knot timeline, local search, cached reads and queued outbox; ActivityPub resolution and attested content. Sources: `.agent/brief.md`, `.agent/brief-mastodon.md`, `apps/reed/core`, and protocol collections. |
| Loomed | Identity generate/import; circles, member wraps, sealed text and image payloads, member sync, receipts, local search, cached history and queued outbox; rejection for outsiders. Sources: `apps/loomed/PRODUCT.md`, its core, and spec chapters 8–10. |
| Weft | Public aggregate counts, advertised-peer topology, DID/knot lookup, handle resolution fixtures, unreachable peers, private-data exclusion, and read-only behavior. Sources: `apps/weft/PRODUCT.md`, its core, and `node.stats`. |
| Provider | Signup/login/logout, invalid and unauthenticated access, public posting and custody disclosure, restart durability, private archive import/search/download, isolation, scoped ingestion/revocation, recovery, and destination-bound account/archive migration with interrupted acknowledgement. Sources: provider README and `docs/features/social-imports.md`. |
| Protocol | Describe/profiles/stats; SIWS; signed records and malformed writes; visibility and capabilities including expiry/revocation; blobs; public/authorized incremental CAR sync and import; concurrent heads, restart durability, WebSocket subscribe/replay, public overlays, identity/location migration, and local ActivityPub gateway fixtures. Keep cryptographic vector, MLS, page CRDT, and resolver coverage explicitly linked to their existing tests. Sources: `docs/spec/00`–`docs/spec/21`, OpenAPI, vectors, and knot/client READMEs. |

Do not claim a public Nostr/AT gateway, real third-party social login, full
historical source coverage, native OS notifications, or unimplemented APIs.
Record any discrepancy between a stated feature and current behavior. Avoid
turning this test task into unrelated product implementation without evidence
of a necessary fix.

## Cluster and lifecycle

- Three isolated persistent knots with distinct identities and advertised
  peer URLs. Two provider hosts for the provider move scenarios. Start only
  the processes needed by a scenario.
- Bind loopback on ephemeral ports and use a fresh temporary root per run.
  Do not access `.data`, `data-provider`, home-directory identities, or
  production services.
- Use the real `tangled-knot` binary. For provider port allocation, prefer a
  test-only worker using the production provider router with a pre-bound
  listener rather than an unchecked free-port reservation race.
- Explicit readiness deadlines and request/scenario timeouts. Check process
  exits while waiting and retain bounded logs on failure. Own and reap every
  child, proxy task, and sampler even if startup or an assertion fails.
- Local fixtures only. No external credentials or source-service requests.
  Tests must not imply peer advertisement replicates records automatically:
  exercise the actual client pull/import paths.
- Real application core operations and replica observations provide the app
  acceptance boundary for the proposed approach. Name this boundary in docs
  and reports so it is not mistaken for browser E2E coverage.

## Metrics and measurement boundaries

| Metric | Definition |
| --- | --- |
| State growth | Sum regular-file lengths in each node data directory and client replica before/after a measured phase; report absolute sizes and signed deltas separately, plus record/blob counts. File lengths are logical disk bytes, not filesystem allocated blocks. |
| Bandwidth | Count bytes in each direction at instrumented TCP forwarding boundaries used by all cluster clients/advertised URLs. Include HTTP headers and bodies and WebSocket bytes traversing those boundaries; exclude TCP/IP framing and TLS overhead. Report upload/download bytes and bytes/second. Count each ingress boundary once; do not add both ends of the same transfer. |
| Processing wall time | Monotonic elapsed time from workload submission until asserted destination materialization. Report operation latency distribution and throughput. Label this end-to-end processing latency; do not mislabel it as server CPU time. |
| Memory | Sample resident set bytes for each child and the workload driver on Linux/macOS. Report baseline, observed sampled peak, and end RSS, sampling interval, and sample count. Report the simultaneous sum separately from the sum of per-process peaks. An unavailable sample is an explicit error/null with a reason, never zero. |

Startup, fixture setup, readiness traffic, warmup, and measured work have
separate phases. Reports state whether polling and background traffic are
included. Keep metric collection outside the hot request path where possible.
Resource-intensive provider password hashing must be labeled separately from
steady-state posting/sync measurements.

## Benchmark execution and artifacts

- A small deterministic smoke workload runs in CI. Correctness assertions
  are mandatory in benchmark mode too.
- An explicit release benchmark command supports dataset size, payload bytes,
  warmup count, and repeated independent runs. Defaults remain small enough
  for a developer machine. Do not introduce absolute speed/RSS gates tied to
  this machine.
- Emit versioned JSON with per-scenario/per-node metrics and a readable summary.
  Include git revision and dirty status, build profile, OS/architecture,
  workload parameters, units, successful operation counts, and failures.
  Repetition summaries include median and p95 with the percentile convention
  stated. Do not pool unlike operations.
- Failed scenarios return a nonzero status and preserve a failure artifact;
  never publish a success benchmark for incomplete work.
- Document one command for acceptance, one for release benchmarks, and how
  to compare reports. CI uploads reports and failure logs as artifacts.

## Verification and limits

Verify metric collection using controlled byte transfers, state writes, and
process sampling; verify lifecycle teardown on failed readiness and failed
scenarios. Exercise required scenarios over actual sockets, then run relevant
workspace tests and formatting/lint checks. Run the release benchmark and
report actual results with the chosen workload and machine context.

Acceptance is a feature-mapped passing cluster suite, reproducible benchmark
artifacts for all four requested metric families, and accurate coverage docs.
Existing unit/vector tests remain necessary for cryptographic conformance;
they do not become cluster tests merely by being listed in a manifest.

Browser UI flows are outside this suite's acceptance boundary, as selected
by the user. Routine implementation choices are left to the builder within
the measurement and lifecycle constraints above.
