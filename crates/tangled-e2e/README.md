# Cluster acceptance and resource benchmarks

The suite runs production Rust app cores over actual loopback HTTP to isolated
`tangled-knot` subprocesses. Provider scenarios start two additional subprocesses
with the production provider router. Temporary identities/data are removed and
children are killed/reaped on exit. No public service or account login is needed.

Five executable feature specifications live in [`docs/spec/e2e`](../../docs/spec/e2e/):
Reed, Loomed, Weft, provider, and protocol. Their stable IDs must exactly match
the scenario results. Cryptographic vectors and other lower-level tests are
documented separately. Browser, codec playback, native notification, and real
third-party source-login coverage are outside this suite.

```sh
just test-e2e
just bench-e2e
```

Equivalent commands, from the workspace root:

```sh
cargo build -p tangled-e2e -p tangled-knot
cargo test -p tangled-e2e
target/debug/tangled-e2e --output target/e2e/acceptance.json

cargo build --release -p tangled-e2e -p tangled-knot
target/release/tangled-e2e --benchmark --operations 10 --payload-bytes 256 \
  --warmups 1 --repetitions 3 --output target/e2e/benchmark.json
```

Use `--scenario reed|loomed|weft|provider|protocol|throughput` to isolate a
workflow. `all` is the default; benchmark mode also adds the dedicated
publish/pull/import/materialize workload. Each iteration starts a fresh cluster.
Warmups use separate clusters and remain in JSON but never enter summaries.
Benchmark mode requires a release build. `TANGLED_KNOT` and `TANGLED_E2E_BIN`
can override executable paths for diagnostics; keep both in the same profile.

`--operations` accepts 1–10000, `--payload-bytes` accepts 1–16384, repetitions
1–100, and warmups 0–20. These are bounded local workloads, not capacity claims.
Surface scenarios include fixed correctness checks in addition to configurable
work; provider text beyond its API's 2000-byte limit is split into multiple posts.

Reports use schema version 1 and are written progressively/atomically. A failed
assertion, timeout, sampling failure, or coverage omission exits nonzero and
sets `status: failed`. Process logs are retained beside the report. Reports
include revision/dirty status, build profile, machine, workload, per-directory
state deltas, per-node public record/blob counts, byte counts, memory samples,
individual operation latencies where recorded, and repetition summaries.

Metric definitions:

- **State:** regular-file lengths under each node, replica and recovery
  directory, with before/after/signed delta. Symlinks are not followed and
  harness logs are excluded. These are logical bytes, not allocated disk blocks.
- **Bandwidth:** upload/download bytes read at the TCP forwarding boundaries
  in front of each node, including HTTP and WebSocket data. Readiness traffic
  is excluded from the measured delta. TCP/IP framing, TLS, and unproxied
  local AP/SNS fixture exchanges are excluded. Rates use measured workflow
  duration. Existing node breakdowns and total proxy traffic are both reported;
  the total also includes transient fixture nodes.
- **Time:** monotonic elapsed time for each full asserted workflow. Fixed
  correctness fixture work is included; provider signup/password hashing,
  archive import, recovery KDF, and migration are included in the provider
  workflow. This is not steady-state post latency. The separate throughput
  workload records every signed publish → incremental CAR pull → import →
  destination-read assertion, and reports successful operations/second.
- **Memory:** RSS in bytes using `ps` on Linux/macOS for all registered node
  processes and the driver. Target sampling interval is 100 ms plus sampling
  overhead. Baseline, observed peak, end, and sample count are reported per
  process name across managed restarts. A new fixture process's baseline is
  its first observation. Expected restart intervals may have no sample;
  unexpected/unavailable samples fail the run. Cluster+driver summed peaks
  use each sampling pass, rather than adding independent process peaks.
  Sequential `ps` queries approximate simultaneous observations; short spikes
  can be missed. Probe overhead is part of the benchmark environment.

Percentiles use nearest rank (p50 is the lower median for even counts).
Summaries exclude warmups and failures and never combine different surfaces.
Compare reports using identical profile, hardware, workload size, payload,
warmup/repetition counts, and fixture boundaries. Do not treat the CI machine's
wall time or RSS as an absolute performance threshold. Increase operation
counts to distinguish fixed feature-check overhead from state growth.

The CI workflow runs smoke acceptance on pushes/PRs, uploads JSON and logs,
and offers release benchmarks on manual dispatch. Run the rest of the
workspace separately with `cargo test --workspace`; cluster acceptance does
not replace protocol vector/MLS/page-CRDT tests.
