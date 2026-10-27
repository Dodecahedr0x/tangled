# Protocol cluster acceptance

These required IDs run in `tangled-e2e` against three persistent, isolated
`tangled-knot` subprocesses over loopback HTTP/WebSocket connections. A feature
ID is returned only after its assertions pass. The coverage gate compares this
table with the run results. Benchmark mode retains the same assertions.

| ID | Observable acceptance | Scenario | Source contract | Coverage |
| --- | --- | --- | --- | --- |
| PROTOCOL-DESCRIBE-STATS | Describe returns protocol version, operator DID, public profile and configured peer URLs; stats identify the node and count accepted writes. | `protocol::run` discovery and signed workload | [Architecture](../03-architecture.md), [HTTP](../12-http-api.md) | Cluster required |
| PROTOCOL-SIWS | Challenge binds the requesting DID; invalid signature is rejected; a real SIWS cookie authorizes a guest-signed write that can be read back. | `protocol::run` session checks | [Cryptography](../05-cryptography.md), `tangled::session` | Cluster required |
| PROTOCOL-SIGNED-RECORDS | Configured public payloads roundtrip with matching returned CIDs; tampered signatures, missing signatures and unknown visibility fail. | `protocol::run` signed workload and malformed writes | [Data model](../06-data-model.md), [Cryptography](../05-cryptography.md) | Cluster required |
| PROTOCOL-VISIBILITY-CAPS | Public reads succeed; capability and sealed records reject anonymous reads; audience-signed DAG-CBOR invocations reveal the exact authorized body; stolen audience authority fails. | `protocol::run` visibility matrix | [Visibility](../08-visibility.md), [Capabilities](../09-capabilities.md) | Cluster required |
| PROTOCOL-CAP-EXPIRY-REVOCATION | Expired grants fail; read grants cannot authorize writes; published revocations invalidate successful repo and circle read grants. | `protocol::run` expiry, confused authority and revocations | [Capabilities](../09-capabilities.md) | Cluster required |
| PROTOCOL-BLOBS | Uploaded public bytes roundtrip after a public reference; sealed blob references require circle authority and return exact bytes only when authorized. | `protocol::run` blob matrix | [Visibility](../08-visibility.md), [Data model](../06-data-model.md) | Cluster required |
| PROTOCOL-CAR-SYNC | Public CAR excludes private record blocks; authenticated import materializes public records on another node. Commit-CID incremental pull contains the since boundary and new head, excludes older commits, and imports against locally held history. Supplemental record-CID pull omits old payloads. Authorized CAR includes a granted private record and imports it into a third node; circle-scoped CAR excludes other circle payloads and logs. | `protocol::run` explicit pull/export/import | [Repositories](../07-repositories.md), [Visibility](../08-visibility.md) | Cluster required |
| PROTOCOL-CONCURRENT-HEADS | A delegated signer updates a shared page; a sibling update against the same predecessor returns `ConcurrentHeads` and preserves the first accepted value. | `protocol::concurrent` | [Data model](../06-data-model.md), [Repositories](../07-repositories.md) | Cluster required |
| PROTOCOL-RESTART | Process restart with the same temporary data preserves head commit, public record, public blob and read-capability revocation. | `protocol::run` restart | `tangled-knot` persistence and [Repositories](../07-repositories.md) | Cluster required |
| PROTOCOL-WS | Live subscription observes the submitted commit and operation; reconnect with `since` replays it; anonymous sealed-collection subscription rejects the upgrade. | `protocol::sockets` | [Synchronization](../11-sync.md), `tangled-knot/tests/ws.rs` | Cluster required |
| PROTOCOL-OVERLAYS | Empty backlink results do not claim completeness; backlinks and loom queries include public annotations and exclude capability-private annotations. | `protocol::overlays` | [Overlays](../14-looms.md), [Visibility](../08-visibility.md) | Cluster required |
| PROTOCOL-LOCATION-DEVICE | Imported identity history remains readable at the destination; a signed location record names its endpoint; delegated device writes succeed before revocation and fail afterward without materializing the rejected record. | `protocol::migration` | [Identifiers](../04-identifiers.md), [Capabilities](../09-capabilities.md) | Cluster required |
| PROTOCOL-ACTIVITYPUB | Local actor/WebFinger/outbox resolve produces the expected DID, attested foreign record and public post; signed local inbox delivery materializes a post; an unsigned inbox request is rejected. | `protocol::activitypub` | [Gateways](../16-interoperability.md), `tangled-knot/tests/ap.rs` | Cluster required |

The workload uses the real current time for signatures, grant validity and
invocations. The requested operation count controls the signed public-record
workload and payload bytes control its text size; fixed acceptance fixtures
run in addition. Peer advertisement alone never counts as replication: CAR
transfers and imports are explicit. The tests use node operators and generated
local identities, temporary data directories and local ActivityPub fixtures.

The normative incremental CAR scenario uses a commit-CID cursor, checks its
chain boundary and imports using verified destination history. A separate
record-CID convenience pull checks payload omission. Commit-CID cursors also
control WebSocket replay; the suite does not claim that commit-CID CAR cursors
minimize transferred record payloads.
Location coverage verifies a signed endpoint update and history at the
destination; it does not claim an automatic old-host redirect or DNS migration.

## Supplemental unit and vector coverage

These tests remain separate from cluster acceptance. Listing them does not
mark a required cluster ID passed.

| Supplemental scope | Existing evidence |
| --- | --- |
| Signature, canonical DAG-CBOR, CID, HPKE, capability chains, MST proofs and protocol fixture conformance | `crates/tangled/tests/vectors.rs`, [vector manifest](../vectors/README.md) |
| MLS welcome, commit and epoch key processing | `crates/tangled/src/mls.rs` tests; `mls-welcome-commit.json` vectors |
| Page CRDT concurrent operations and compaction admissibility | `crates/tangled/src/page.rs` tests; `page-crdt-concurrent.json` and `page-crdt-compact.json` vectors |
| Handle DNS/SNS parsing, resolution and identity key separation | `crates/tangled/src/handle.rs` and `identity.rs` tests; `handle-dns.json` vectors; `tangled-knot/tests/persist.rs` handle fixtures |
| Capability caveats, maximum use counters across restart and protocol edge cases | `crates/tangled-knot/tests/http_api.rs`, `persist.rs`, `crates/tangled/tests/e2e_protocol.rs` |
| ActivityPub RSA signature edge cases and actor-key matching | `crates/tangled-knot/tests/ap.rs`; `crates/tangled-ap` unit tests |

Browser onboarding, live public federation, public Nostr/AT gateways and
third-party credentials are outside this local Rust/HTTP acceptance boundary.
