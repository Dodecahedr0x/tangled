# Hosted provider cluster acceptance

These required features run in `tangled-e2e::scenarios::provider` against two
isolated provider subprocesses, using the production provider and archive routers
over counted loopback HTTP. This is Rust HTTP acceptance, not browser coverage.
Every ID is returned only after its assertions complete; the coverage gate must
reject a missing ID.

| ID | Source contract | Observable acceptance | Scenario |
| --- | --- | --- | --- |
| provider-auth | `crates/tangled-provider/README.md`; `/signup`, `/login`, `/logout` | Signup and password login issue sessions; logout invalidates the old session; malformed signup, duplicate email, wrong passwords and unauthenticated writes/archive reads fail. | `run`: authentication |
| provider-public-posts-custody | Provider README; `/posts`, `tangled.node.describe` | Each submitted public post appears to a guest; describe advertises custodial signing and the archive page discloses provider custody. Payloads over the posting API's 2000-byte bound are split into separately verified posts. | `run`: public posting |
| provider-restart-durability | Provider README; persistent provider stores | After subprocess replacement, password login, session-backed archive search, original download and public history still work. Destination history/archive also survive replacement after moving. | `run`: source and destination restart |
| provider-private-archive | `docs/features/social-imports.md`; multipart upload, archive query, original download | A local Telegram export with the requested message count/payload is uploaded; each message's full text/source data and the export metadata are preserved and searchable. Unmatched search and public pages exclude it; original download is byte-for-byte. A second account and unauthenticated clients cannot read its original or objects. | `run`: archive import and isolation |
| provider-scoped-ingestion | Social imports guide; connection/ingest/disconnect endpoints | A connection token ingests matching source/account activity, rejects invalid tokens and other source/account batches, cannot read the archive, and is unusable after owner disconnect; retained activity remains searchable. Another account cannot disconnect it. | `run`: scoped connector |
| provider-recovery | Social imports guide; encrypted recovery export and `tangled_import::recovery_open` | HTTP exports an encrypted recovery packet. Wrong passphrase and owner tampering fail. Offline decryption and restore into a fresh archive recover the owner, identity, objects and exact original; connector tokens are excluded. | `run`: recovery export and offline restore |
| provider-destination-bound-move | Provider README; v2 encrypted account/archive handoff | Prepared HTTP handoff excludes plaintext custody/archive material; changed recipient key or destination is rejected. Unrelated keys and changed associated data cannot decrypt it. The same destination reproduces the sealed handoff after source restart; bad acknowledgement fails and correct acknowledgement is idempotent. | `bound_handoff` |
| provider-account-archive-move | Provider README and social imports guide; `/move` | Two real hosts transfer the same DID, public history, private activity, original and custody keys. The destination can sign new posts; source sessions cannot sign/read the private archive; old connector tokens are revoked and destination connections require fresh authorization. | `run`: two-host move |
| provider-interrupted-ack | Social imports guide; destination acknowledgement and resume | An owned local fault proxy rejects the first source acknowledgement after destination persistence. Both hosts restart; a wrong destination password cannot resume. The correct password resumes the saved receipt and completes the move, preserving archive bytes and identity. | `run`: interrupted acknowledgement |

Telegram exports retain an `archive_metadata` object as well as message objects.
Search can match the embedded source messages in that metadata; acceptance checks
message kinds and exact content, and verifies metadata separately. While a move
awaits acknowledgement, a prepared source session can still render account UI;
posting returns 502 with the signing-fence explanation and private archive access
returns 401. The scenario verifies those observable restrictions.

Signup/password hashing and recovery KDF work are included in this scenario's
measurement and must be labeled separately from steady-state posting/sync.
Provider subprocess setup happens in `setup` before measurement. The local
acknowledgement fault proxy forwards to the normal counted source boundary;
its injected rejection is fixture traffic. The offline recovered archive lives
under the run's temporary root and is client-side recovery state.

Existing supplemental tests remain separate from cluster acceptance:

- `crates/tangled-provider/tests/onboarding.rs`: in-process router onboarding.
- `crates/tangled-provider/tests/move.rs`: source custody erasure in SQLite.
- `crates/tangled-provider/tests/import_migration.rs`: repeated location moves,
  retained/wiped seed inspection and destination receipt database assertions.
- `crates/tangled-knot/tests/imports.rs` and `crates/tangled-import/tests/`:
  archive routing, parser fixtures, encryption, recovery and store invariants.

These fixtures do not claim real Telegram/Twitter/WhatsApp authentication,
connector execution, full historical exports, email delivery or browser UI.
