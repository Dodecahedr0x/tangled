# Reed application-core cluster acceptance

Boundary: production `reed-core` operations and persisted replicas over HTTP to
three subprocess knots. These are not browser or native notification tests.
The scenario returns its feature IDs only after every assertion succeeds.
Contract sources: `apps/reed/core`, `.agent/brief.md`,
`.agent/brief-mastodon.md`, and the public post/profile/social collections.

| ID | Observable acceptance | Test/scenario |
| --- | --- | --- |
| REED-identity | Generated identity exports/imports the same signing DID and agreement key; a bare Solana array is rejected; imported credentials authenticate a real knot session. | Cluster: `apps::reed` |
| REED-text | Configured operation count and text payload upload and materialize unchanged in a fresh replica. | Cluster: `apps::reed` |
| REED-reply-quote | A pulled reply retains its parent; a quote retains its quote reference and does not become a reply. | Cluster: `apps::reed` |
| REED-edit-delete | Pulled edited text replaces the draft; a deletion materializes a tombstone and disappears from discovery. | Cluster: `apps::reed` |
| REED-follow | Published follow includes the remote knot, remote author enters home, and unfollow removes it after another pull. | Cluster: `apps::reed` |
| REED-social | Like/boost and their undo operations materialize correct flags/counts; publish/pull mute/block hides a followed remote author and undo restores it. | Cluster: `apps::reed` |
| REED-profile | A fresh replica reads the published display name after HTTP sync. | Cluster: `apps::reed` |
| REED-media | Image/video post metadata and both uploaded blobs survive HTTP round trip; pulled bytes are available in the local cache. | Cluster: `apps::reed` |
| REED-search | Case-insensitive local search finds exactly all configured workload posts in the synced replica. | Cluster: `apps::reed` |
| REED-offline-outbox | Failed text record upload retains pending work, cached authored text survives replica reopen, and reconnect drains exactly the queued record. | Cluster: `apps::reed` |
| REED-cross-knot | Bob publishes on a distinct persistent knot; Alice's actual core follow/pull path imports Bob into her home timeline. Advertisement alone is not treated as replication. | Cluster: `apps::reed` |
| REED-activitypub | Existing `seed::run_ap_mock` sends signed Notes from a loopback AP peer through the production inbox, resolves/follows its account, and Reed pulls attested posts with source URLs. | Cluster: `apps::reed` |

The offline retry claim covers text records. Reed's blob queue currently removes
the first blob before attempting upload; this scenario does not claim a failed
media upload is safely retried. Image/video fixtures are opaque deterministic
blob bytes; codec playback belongs to UI coverage.

Supplemental existing coverage, separate from cluster acceptance:
`apps/reed/core/tests/core.rs` covers parser edge cases, keystore, media limits,
mentions, inbound social lists and mock-engine behavior. Cryptographic vectors
remain in `crates/tangled/tests/vectors.rs`.
