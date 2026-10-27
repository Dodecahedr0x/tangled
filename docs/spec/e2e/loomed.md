# Loomed application-core cluster acceptance

Boundary: production `loomed-core` over a real subprocess knot, with distinct
owner, member and outsider identities and independently persisted replicas.
Contract sources: `apps/loomed/PRODUCT.md`, `apps/loomed/core`, and protocol
circle, capability and sealed-record chapters. IDs return only after assertions.

| ID | Observable acceptance | Test/scenario |
| --- | --- | --- |
| LOOMED-identity | Generated identity imports the same DID/agreement key; bare keypair import fails; owner/member/outsider authenticate real HTTP sessions. | Cluster: `apps::loomed` |
| LOOMED-circle-wraps | Public DM circle names owner/member, excludes outsider, and a production HPKE member wrap permits decryption after HTTP pull; a separate three-person group delivers to both members while its third member cannot read the DM. | Cluster: `apps::loomed` |
| LOOMED-sealed-text | All configured sealed text records are published and a fresh member replica decrypts their original text/IDs. Public CAR contains no sealed records. | Cluster: `apps::loomed` |
| LOOMED-sealed-image | A sealed image message decrypts to the uploaded image CID; an explicit owner-signed `blob/get` grant to the member downloads the original fixture bytes over authorized HTTP. | Cluster: `apps::loomed` |
| LOOMED-member-sync | `Sync::http().poll_once` discovers membership, obtains a circle grant/wrap and materializes owner messages for a fresh member replica. | Cluster: `apps::loomed` |
| LOOMED-receipts | Member publishes a sealed receipt over HTTP; owner explicitly pulls the member's repository using its signed circle capability, opens the receipt with the production parser and materializes member acknowledgement on the target. | Cluster: `apps::loomed` |
| LOOMED-search | Case-insensitive search of decrypted member history finds exactly the configured workload text messages. | Cluster: `apps::loomed` |
| LOOMED-offline-outbox | Failed upload keeps queued records; owner history survives reopen; reconnect uploads the queue, and member plaintext history survives reopen. | Cluster: `apps::loomed` |
| LOOMED-nonmember-deny | A distinct authenticated outsider receives no decrypted messages; authorized repo endpoint rejects its request without a circle grant with HTTP 401/403. | Cluster: `apps::loomed` |

Image acceptance covers the sealed message/reference and blob integrity. The
current core uploads image bytes as ordinary blobs; this is not a claim that
the blob bytes themselves are encrypted. Automatic image grant distribution by
the application is not claimed. Receipt acceptance deliberately uses
an explicit authorized pull of the receipt author's repo: automatic multi-author
receipt discovery by `Sync::poll_once` is not claimed.

Supplemental existing coverage, separate from cluster acceptance:
`apps/loomed/core/tests/core.rs` covers group construction, mock-engine behavior,
sealed parser/receipt edge cases and local key storage.
`apps/loomed/core/tests/knot_live.rs` covers a separate two-member live-knot DM.
MLS/HPKE conformance and cryptographic vectors retain their existing protocol
unit/vector tests; this suite does not turn those into cluster claims.
