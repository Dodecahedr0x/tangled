# Social activity imports

Status: builder-reviewed proposed design; hybrid approach selected by the user, design approval pending.
Date: 2026-10-27.

## Intent and agreed scope

Tangled should aggregate a person's activity from other networks into their own account, whether they run a knot themselves or use a hosted provider. The user requested WhatsApp, Twitter/X, Telegram, and extensibility to other apps, and clarified that imports should capture as much as possible. They selected historical archive imports plus ongoing account connections.

This includes authored and received content: posts, replies, direct messages, group histories, channels, attachments, voice notes, reactions, contacts, follows, bookmarks, profile information, and other activity that the connected account or supplied export actually exposes. These are coverage targets, not a promise that every source exposes every category. Preserve unsupported source data rather than discarding it because no Tangled view exists yet.

Success means a person can connect or import a source, browse its available content in Tangled, refresh without duplicates, see gaps and errors, disconnect, and move their archive to another host. Historical records retain their actual senders and dates.

## Approaches

1. Archives first: broad historical coverage without persistent source credentials; requires repeated uploads to refresh.
2. Live connections first: automatic incremental updates; historical coverage and data categories are restricted by source APIs and access permissions.
3. Hybrid, selected by the user: archive imports fill history and category gaps; authorized connections fetch ongoing activity into the same archive. More adapter work, but the best fit for maximum coverage.

The remaining implementation choices below are recommendations made by the author, not additional user decisions.

## Current repository and implications

- `crates/tangled` provides signed records, blobs, capabilities, sealed data, CAR export/import, and foreign-object identifiers and duplicate detection.
- `spec/16-interoperability.md` defines provenance envelopes and idempotence for existing protocol bridges. These ideas are reusable; an uploaded archive is not independently signed evidence of a foreign author's identity.
- `crates/tangled-knot` is the HTTP server. The provider wraps this router with account/session management and custodial signing.
- `crates/tangled-provider` currently creates public posts. It has no source-connection management, import jobs, or private-archive experience.
- Reed consumes public social records, Loomed consumes sealed circle messages, and Weft explores the network. An imported conversation cannot simply become a native circle of invented Tangled identities.
- The provider's `finish_move`/`accept_move` currently transfer public records and rotate operational signing/agreement keys. This is insufficient for sealed archives; migration changes are required before shipping private imports with a working Change host action.
- The checkout contains substantial existing uncommitted application/provider changes. Implementation must preserve those changes and avoid committing unrelated files.

## Product rules

- Import is read-only with respect to the source account. It does not send messages, publish posts, acknowledge reads, follow people, or join groups. Required authentication/protocol maintenance traffic is allowed; content-changing behavior is out of scope.
- Import all supported categories by default within the access the user grants. Offer exclusions, date filters, and media/storage limits, with an explicit summary of what those limits omit.
- Store the imported archive privately by default, including copies of public source posts. Importing does not republish them to Tangled's public network.
- Foreign senders remain source identities inside the archive. Never mint their Tangled DIDs or claim they signed the imported records. Importing a conversation does not invite or notify its participants.
- Mark imported content with its source and distinguish uploader-supplied data, authenticated retrieval, and independently verifiable source signatures. Authenticated retrieval is not an author signature.
- Disconnect stops collection and removes retained source credentials. Keep the archive unless the owner separately deletes it. A session revoked at the source must stop collection too.
- Source edits/deletions are recorded as events when observed. Preserve earlier imported versions in this personal archive, label deleted/expired items, and offer owner deletion. Do not interpret an absent page or failed fetch as deletion. Make retention of imported disappearing-message content clear when connecting.
- Preserve source relationships; importing an X follow must not automatically create a native Tangled follow. Cross-posting, replying through Tangled to the source, and automatic identity merging are outside this design.

## Source coverage and authentication

| Source | Historical route | Ongoing connection | Coverage and constraints |
| --- | --- | --- | --- |
| Telegram | Telegram Desktop JSON export with its media directory; account takeout for authenticated backfill | User-account authorization through Telegram's client API, not a bot token | Cloud chats, groups/channels accessible to the account, media, contacts, and other takeout categories. Report unavailable data and media explicitly. |
| Twitter/X | Official account archive, including its JSON/JavaScript data files and media | User-authorized X API; endpoint availability, permissions, and operator-configured budget determine categories | Posts, replies, DMs, relationship/activity data and media where supplied. Archive files are data: never execute their JavaScript wrappers. |
| WhatsApp | Exported individual/group chats, with available attached media | An optional personal-account linked-device adapter with QR/pairing login | Personal accounts need a linked-device integration rather than substituting the Business API. History and attachment availability vary; exports supplement connection backfill. Label this adapter experimental until its pairing, reconnection, and history behavior are verified. |

WhatsApp live support is part of the target, not silently reduced to uploads. A maintained linked-device library is the proposed starting point; isolate it as a replaceable sidecar if its language/runtime differs from Rust. Do not add Matrix as a requirement merely to reuse a Matrix bridge. The exact library is a builder choice after checking maintenance, license compatibility, supported operations, and history/media behavior. A provider may disable an unavailable adapter, but must show that ongoing sync is unavailable rather than a working Connect action.

No connector may promise access to deleted, expired, inaccessible, or never-exported content. Coverage is a versioned matrix per connector and category: archive available, live available, restricted, unsupported, or failed. Display actual imported date ranges and counts; do not describe an incomplete historical import as complete.

Source evidence checked on 2026-10-27:

- [Telegram Desktop export](https://telegram.org/blog/export-and-more) supports JSON and media exports.
- [Telegram takeout](https://core.telegram.org/api/takeout) documents account data categories and pagination; [user authorization](https://core.telegram.org/api/auth) documents account login.
- [X account archive](https://help.x.com/en/managing-your-account/how-to-download-your-x-archive) documents downloadable account data; [X API access](https://help.x.com/en/rules-and-policies/x-api) documents authorized access to non-public information.
- [WhatsApp export](https://faq.whatsapp.com/1180414079177245/) documents chat export. [Whatsmeow](https://github.com/tulir/whatsmeow) is a candidate linked-device library, not a selected dependency. The [maintainer's bridge documentation](https://docs.mau.fi/bridges/general/backfill.html) demonstrates linked-device history constraints; those are evidence to test, not guarantees for a Tangled adapter.

## Architecture

Add `crates/tangled-import` as the shared Rust import domain: source data types, parsers, normalizers, identity resolution within a source, duplicate handling, provenance, job state, and repository writes. Keep HTTP framework, provider account storage, and UI out of this crate.

Both archive parsers and live adapters feed the same ingestion operation. The host supplies the authenticated owner DID, a scoped signer, blob/repository access, and an archive encryption context. Source credentials cannot select a different destination owner. Never pass an arbitrary DID from an import request directly to a custodial signer.

Mount authenticated import/connection routes in both `tangled-knot` and `tangled-provider` using a shared HTTP module. Provider routes resolve the owner through the existing account session. Self-hosted routes use an identity session or appropriately scoped capability. The host validates destination authorization independently of source login. Expose supported connectors and their modes only after checking configured runtime availability.

Use a persistent SQLite job/connection store and a restartable worker in the host process initially. Source libraries that require a separate runtime communicate through an authenticated local socket; do not expose their administration endpoint publicly. A source-specific failure must not block imports from other sources. Avoid an external queue dependency for the first implementation.

Separate public hosting from private import execution: the importer is an authorized client component that can see source plaintext and archive secrets. A self-hosted importer may run alongside the knot. A hosted importer runs under the provider's custody and therefore can read imported content; disclose this explicitly and do not claim secrecy from the provider. Sealed repository storage protects public replication and other accounts, not a provider running the importer and holding the keys.

## Portable archive model

Add versioned application schemas, published as `sys.schema` records, rather than changing the frozen v1 core schemas:

- `app.tangled.import.account`: stable source-account identifier, aliases, profile, source and connector version, capture provenance, and coverage. Never contains login secrets.
- `app.tangled.import.conversation`: source-scoped conversation identity, title/type, participants as foreign identities, source relationships, and coverage.
- `app.tangled.import.object`: normalized imported activity plus references to original bytes, attachments, conversation, related objects, provenance, versions, and availability.
- `app.tangled.import.batch`: archive manifests and durable receipts of captured objects, source files, parser version, byte counts, and omissions. Live fetch batches use the same receipt model.

All four collections are sealed, owner-only application data. The signed record's `author` is the archive owner/importing authority; its encrypted body identifies the foreign actor. The outer `ts` represents the local write; original creation/edit timestamps, source timezone/precision, and capture time are distinct encrypted fields. Views sort historical content by source time with a stable tie-breaker, not import order. Preserve an uncertain timestamp as uncertain.

Each normalized object carries `source`, stable `source_account_id`, `kind`, source object/conversation/actor identifiers when known, content and structured entities, original timestamps, source revision information, origin URL when available, raw-content references, media status, and an authenticity classification. References use canonical source identities so unresolved replies can be filled later without inventing a native CID.

Preserve every supplied source file as encrypted blob chunks referenced by an encrypted manifest, including unknown categories and unrecognized fields. Store normalized activity as a projection of that data, not its replacement. For live imports, preserve exact response/event bytes and download available media. An unsupported category is retained and marked not yet browsable; a corrupt file is identified without silently accepting partial parsing. Preserve original source filenames inside encrypted manifests, not disk paths derived from uploaded names.

Raw files and large media are streamed in chunks below the destination's advertised `maxBlob`; record manifests preserve ordering, sizes, media types, integrity hashes, and encryption metadata. Missing media remains an explicit placeholder with a reason. Do not pretend a remote URL is a durable imported attachment.

Existing `sys.foreign` bridge behavior stays compatible. Private source identifiers/provenance must not be exposed through a new public `sys.foreign` envelope merely to reuse its schema. The new archive follows the same canonical-ID and retained-bytes principles, with application-specific schemas and encrypted metadata.

## Encryption and access

Use one owner-only archive circle and the existing sealed-record/epoch-key mechanisms; foreign group participants are not circle members. Source-specific groups are encrypted conversation metadata within that archive. Wrap archive epoch secrets to authorized owner devices using the existing agreement-key mechanisms. Do not grant public collection readers access to the archive.

Use existing sealed-rkey handling for stable opaque storage keys. Source identifiers, names, conversation titles, URLs, filenames, and plaintext hashes belong inside ciphertext. Encrypt attachments and raw source chunks before storing them as blobs; never reference an imported plaintext blob from a public record. The encryption design must ensure unique nonces for different payloads and revisions, including retries and concurrent workers, using existing protocol constructions where applicable.

Keep tokens, source sessions, pending login challenges, and protocol device keys out of records, CAR exports, logs, and source-preservation blobs. Store them in a separate encrypted credential store protected by a configured host secret; temporary uploads and worker state must not leave unprotected plaintext files behind. Do not log message bodies or credential-bearing URLs. Private search indexes are account-scoped; they inherit the custody disclosure and are never included in public indexes or Weft's public graph.

## Identity, duplicates, and revisions

Use the canonical source account ID rather than a mutable handle or connection ID. Reconnecting the same account or adding its archive attaches to that stable account; multiple accounts on the same service remain separate.

When native IDs exist, the logical key is `(source, source_account_id, object_kind, conversation_scope_if_needed, native_object_id)`. Canonicalize source aliases only from authoritative mappings, such as known WhatsApp identifier migrations. The stored sealed rkey derives from that stable key. File/batch IDs and import timestamps are not object identity. Maintain an account-local mapping so the same item arriving through an archive and a live event occupies one logical object.

When an export omits native IDs, derive a documented fallback fingerprint from available conversation, sender, exact timestamp representation, original content, and attachment identities. Identical repeated messages must retain separate occurrences. Reconcile overlapping exports only when occurrence/order context supports a unique match; keep uncertain candidates and show possible duplicates instead of discarding distinct messages. Do not deduplicate by text alone.

Unchanged repeats do not rewrite a record. Verified edits produce a same-key revision with `prev` pointing to current causal heads. Preserve raw captures in batch manifests even if a duplicate normalized object needs no revision. Source-native revision order wins when comparable; an older archive must not overwrite a newer live edit. When order cannot be established, keep both captures and their uncertainty. Connector/parser upgrades can rematerialize projections from preserved bytes without minting new source objects.

Commit blob persistence and record writes before advancing the durable source cursor. Make page/event replay safe after crashes; if the repository and job DB cannot share a transaction, use durable intents and idempotent replay. Only one ingestion lease may advance a given source account's cursor at a time; lost leases must not permit stale workers to advance it. Concurrent archive jobs still use the same canonical mapping and conflict handling.

## Connections and jobs

Connection state distinguishes authentication, backfill, live sync, pause, rate limiting, expired credentials, unavailable adapter, and disconnect. Separate capability/coverage from connection health: connected does not mean full history imported.

Persist a connection ID, owner, stable source-account identity after authentication, configured categories, credential reference, adapter version, pagination/update cursors, and backfill/live status. Persist jobs with progress, counts, media bytes, retries, checkpoints, and actionable failures. Mark a job complete only for its declared scope and report unsupported, unavailable, and skipped content separately.

Use bounded pagination, concurrency, response sizes, retry backoff, source rate limits, and operator-configured storage/API budgets. Budget exhaustion pauses a job with a visible reason, never quietly truncates it. Pause/cancel/disconnect must stop subsequent writes from already-running workers and invalidate pending credentials; use a connection generation fence checked before committing each ingestion unit.

Archive uploads are resumable and streamed. Reject traversal paths, symlinks, archive bombs, unsafe extraction, and executable content. Parse X archive assignments with a bounded data parser, never a JavaScript runtime. Remote media fetches use validated source URLs, bounded redirects, and private-network protections; a self-hosted knot URL is not permission for an imported attachment URL to access host-local services.

For OAuth-style connectors, bind callbacks to the initiating owner/session and validate state and PKCE as appropriate. Telegram and WhatsApp use their actual account-login protocols rather than being presented as generic OAuth. Ask for additional source permissions only when a category needs them; a denied category does not prevent other granted categories from importing.

## Account experience and application integration

Provide account-level Connections and Imports management for both hosting modes, with source availability, supported categories, archive upload, actual login/pairing, progress, missing data, retry, pause, disconnect, export, and delete actions. Add an owner-authenticated archive browser with one chronological activity view, source/account/category filters, conversations, attachment access, and search. This baseline makes all normalized categories available without requiring every existing app to implement every activity type first.

Reed can show imported posts/activity in a private Imports view alongside the owner's native activity. Loomed can show imported conversations as read-only histories with foreign participants, source timestamps, replies, and attachments. These views read the shared archive model; they must not republish archive records or pretend read-only histories support native messaging. Keep unavailable or unrendered source types accessible through preserved data and coverage reporting.

Weft may expose owner-authorized aggregate import health/counts. Source accounts, relationships, conversations, raw manifests, and media never enter its public graph or public statistics. Imported profiles are not added to public discovery automatically.

Export from the account includes normalized records, encrypted source manifests/chunks, attachments, and the necessary out-of-band owner key backup. Offer an authenticated original-format download by decrypting/reassembling preserved source files. Neither export includes live source credentials.

## Host migration and removal

Extend provider migration to transfer a verified, authenticated CAR including every archive collection and referenced encrypted blob, plus an authenticated encrypted handoff of the archive key material required by the destination. A location-only move should preserve identity/agreement keys as the spec requires; if the custodial flow rotates operational keys, rewrap all retained archive epochs before retiring old keys. Do not discard the old agreement key and then attempt to recover archive data.

Verify destination persistence, source manifests, and decryption of representative items from every archive epoch before finalizing the move and retiring old custody. Migration must be resumable and preserve the DID. During cutover, pause/fence source ingestion so both hosts cannot advance the same source account concurrently. Importing history must not automatically transfer live source login sessions: destination connections require reauthorization, while archived data remains usable.

An archive export alone is not a key backup. State exactly which keys are needed. For self-hosted clients, use the existing account/key backup process, extended with archive keys as necessary.

Owner deletion hides/tombstones the selected imported objects and schedules account-local raw data, attachments, temporary files, search rows, and credentials for cleanup where no retained archive reference requires them. Explain that deleting a normalized item alone cannot remove its copy from a retained original archive; offer deletion of the containing source/batch when originals must also be removed. Prevent replay from resurrecting an owner-deleted source object unless the owner explicitly resets that choice. Copies already exported or retained in operator backups cannot be recalled.

## Delivery boundaries and acceptance

Deliver in increments, with the full feature remaining unfinished until the agreed source modes are exercised:

1. Shared schemas/ingestion, owner authorization, sealed storage, portable manifests, duplicate/revision handling, persistent jobs, and account archive browsing.
2. Working Telegram, X, and WhatsApp archive importers, including supplied media and explicit coverage reports.
3. User-authorized Telegram and X ongoing connections/backfill, and WhatsApp linked-device pairing/backfill/ongoing ingestion. Test actual connector capabilities before advertising them; unavailable credentials or runtime dependencies are reported as incomplete validation.
4. Reed/Loomed private import views, full archive export, source reauthorization after migration, and private-archive migration verification.

Acceptance must cover:

- All three sources import representative real-format fixtures, including received/group activity and media; unsupported data survives original-format export.
- Reimporting, overlapping archives, reconnecting, and archive/live overlap do not multiply objects or collapse legitimate repeated messages. Older archives do not replace newer edits.
- Interrupting/restarting imports replays safely; expired source access, rate limits, missing media, and storage exhaustion have accurate recoverable status.
- Another account, unauthenticated HTTP, public repo sync, blob fetch, subscriptions/firehose, public search, and Weft cannot obtain archive plaintext or sensitive metadata. Authenticated owner access works.
- Disconnect/cancel fences in-flight writes and removes source credentials; owner deletion remains effective on replay.
- Live connectors use real authentication paths and a controlled account validation procedure. Mock tests alone do not establish live compatibility or historical completeness.
- An account with multiple archive epochs moves between hosts and can still browse, download, and decrypt its archive, with no source credentials in the export.
- Existing Tangled public publishing, sealed messaging, bridge idempotence, and protocol conformance remain intact.

## Decisions left to the builder

Choose parser/streaming libraries, SQLite schema and internal Rust traits, sidecar protocol/library where needed, exact authenticated HTTP route names, bounded defaults, and visual treatment using repository conventions. Verify source permissions, API costs, library licenses, and current history/media limits before implementing each adapter; avoid hard-coded coverage promises or purchases without operator configuration. Record deviations that reduce the agreed product scope for user review.

An independent builder review found no important questions left before implementation planning. No additional user-owned requirement is currently unresolved. This document awaits user approval; it is not an implementation-complete claim.
