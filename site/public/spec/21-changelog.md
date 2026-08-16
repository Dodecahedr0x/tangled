---
title: Changelog
slug: changelog
order: 21
---

# Changelog

This chapter records **normative** protocol changes shipped after the first complete v1 draft. It is not a client or product changelog. Informal names in parentheses are not wire identifiers.

Each item states the problem, the change, and the RFC 2119 language. Implementers who already shipped an earlier v1 snapshot MUST apply these rules; `v` on records, commits, and capabilities remains `1`.

## 21.1 Credentialed Cross-Origin Resource Sharing (CORS) preflight

**Problem.** Actual responses already echo `Origin` and send `Access-Control-Allow-Credentials: true` when `Cookie` or `Authorization` is present. `OPTIONS` always sent `Access-Control-Allow-Origin: *`, so browsers rejected credentialed preflight.

**Change.** Preflight uses the same credentialed rule as the actual response, keyed off `Access-Control-Request-Headers`.

A knot MUST send `Access-Control-Allow-Origin: *` on `OPTIONS` when `Access-Control-Request-Headers` does not list `Cookie` or `Authorization`.

When `Access-Control-Request-Headers` lists `Cookie` or `Authorization` (comma-separated, case-insensitive), the `OPTIONS` response MUST NOT use `*`. It MUST echo the request `Origin`, MUST set `Access-Control-Allow-Credentials: true`, and SHOULD set `Vary: Origin`. If that preflight has no `Origin`, the knot MUST omit `Access-Control-Allow-Origin`.

See [HTTP API](./12-http-api.md#cors).

## 21.2 Capability `max_uses` counted

**Problem.** Spec 09 listed `["max_uses", n]` but said a knot MAY skip the counter. The reference knot skipped it, so a one-use grant was reusable.

**Change.** Online knots that accept invocations MUST count successful uses per leaf capability Content Identifier (CID).

A knot that verifies invocations MUST persist a counter keyed by the leaf capability CID. Before authorizing, it MUST reject the invocation with `CapabilityInvalid` (HTTP 403) if the stored count is greater than or equal to `n`. After a successful authorization it MUST increment the counter.

`tangled.cap.verify` MUST NOT increment the counter (it is a check, not a resource use). Offline verifiers MAY ignore `max_uses`. Online knots persist the counter; see [21.18](#2118-max_uses-persisted-across-restart).

See [Capabilities](./09-capabilities.md#caveats).

## 21.3 `listRecords` is not on the wire

**Problem.** The in-process engine served `GET /xrpc/tangled.sync.listRecords`. OpenAPI omitted it. Spec 12 already forbade a wire method. Live knots still answered the path.

**Change.** Confirm the forbid. Clients MUST enumerate with `getRepo`, `getRepoAuth`, or `getRecord`.

A Tangled knot MUST return `MethodNotImplemented` (HTTP 404) for `tangled.sync.listRecords`. An in-process engine MAY keep a `listRecords` helper for tests. A knot MUST NOT require clients to call it.

See [HTTP API](./12-http-api.md#tangled-sync-getrepo).

## 21.4 `getRepoAuth` `since` in query or body

**Problem.** Spec 12 already allowed `since` as a query parameter or in the JSON/CBOR body. The reference knot read only the query string.

**Change.** Honor both. If both are present and differ, the query parameter MUST win.

A knot MUST treat `since` on `tangled.sync.getRepoAuth` as a commit or record CID. It MUST read `since` from the query string when present. If the query omits `since`, it MUST read `since` from the request body when present.

See [HTTP API](./12-http-api.md#tangled-sync-getrepoauth).

## 21.5 KeyPackage records for offline add

**Problem.** Messaging Layer Security (MLS) Add needs a KeyPackage. Spec 10 published Welcomes for offline members but did not say how an adder obtains a KeyPackage when the new member is offline. That forced a round-trip or an out-of-band package.

**Change.** Members publish KeyPackage records in their own repository. Add is still “add member.” Clients MUST NOT expose Hybrid Public Key Encryption (HPKE) versus MLS, or KeyPackage publish, as a user-facing mode.

A `client.group` implementation MUST be able to publish a KeyPackage record:

- collection `sys.mls.keypackage`
- rkey = a Timestamp Identifier (TID)
- `vis` `public` or `cap`
- body `{ type: "keypackage", exp: uint, payload: bstr }`

`payload` MUST be the RFC 9420 TLS-serialized KeyPackage. `init_key` MUST differ from the leaf / `agreement` key. `exp` MUST be Unix seconds. An adder MUST ignore a package with `exp` less than now (the [clock-skew window](#216-capability-clock-skew) applies). An adder MUST Welcome to a current unexpired package when adding an offline member. The new member’s client processes Welcome on next sync. No accept dialog.

See [Circles](./10-circles.md#keypackage-records).

## 21.6 Subscribe `since` cursor

**Problem.** Spec 11 live subscribe had no resume cursor. A dropped WebSocket forced a full `getRepo` before the client could trust it had not missed a commit.

**Change.** Optional `since` query parameter (a commit CID).

`GET /xrpc/tangled.sync.subscribe?did=&collection=&since=` — `since` is OPTIONAL. When `since` is present and the knot still holds later commit events for that Decentralized Identifier (DID) (and `collection` filter), it MUST send those events first, in store order, then continue live. When `since` is omitted, the knot MAY send the current head as a hello with empty `ops` (v1 default). When the knot does not retain `since`, it MUST send that hello and MUST NOT invent a gap-free history.

Subscribe filters remain `did` and optional `collection`. There is no byte-offset cursor. The event log is persisted; see [21.14](#2114-subscribe-since-persisted).

See [Synchronization](./11-sync.md#live).

## 21.7 Blob size and media type on `getBlob`

**Problem.** `putBlob` accepted raw bytes. `getBlob` returned `Content-Type: application/octet-stream` with no size or stored type, so clients could not advertise media without out-of-band metadata.

**Change.** Always send length. Echo a stored type when the writer supplied one.

`putBlob` MAY include `mediaType` as a query parameter or `X-Tangled-Media-Type` request header (a Multipurpose Internet Mail Extensions (MIME) type such as `image/png`). The body `Content-Type` remains `application/octet-stream`. A knot MUST store that type with the blob when present.

`getBlob` MUST send `Content-Length` equal to the byte length. `Content-Type` MUST remain `application/octet-stream` (the CID is raw 0x55). When a media type was stored, the knot MUST also send `X-Tangled-Media-Type` with that value.

See [HTTP API](./12-http-api.md#tangled-sync-getblob).

## 21.8 Capability clock-skew window

**Problem.** Capability `nbf` already existed. Verifiers compared `nbf` and `exp` to `now` with no skew, so a 1-second clock difference rejected a valid grant.

**Change.** A 60-second window on not-before, expiry, and `valid_until`.

Let `SKEW` be `CAPABILITY_CLOCK_SKEW_SECONDS` (60). A verifier MUST accept a certificate when `now + SKEW >= nbf` and `now <= exp + SKEW`. A `valid_until` caveat MUST fail only when `now > until + SKEW`. Implementations MUST use 60. They MUST NOT expose the window as an interop-breaking setting.

See [Capabilities](./09-capabilities.md#invocation) and [Cryptography](./05-cryptography.md#constants).

## 21.9 `Retry-After` on `RateLimited`

**Problem.** Page compact admission already returned `RateLimited` (HTTP 429) with no `Retry-After`, so clients could not back off correctly.

**Change.** Every `RateLimited` response MUST include `Retry-After` as delta-seconds (RFC 9110). For page compact admission the value MUST be at least `PAGE_COMPACT_MIN_INTERVAL_MS / 1000` (300).

See [HTTP API](./12-http-api.md#errors).

## 21.10 Generative vs operational DID `keyAgreement`

**Problem.** A generative `did:sol` document is computed from the Ed25519 public key alone. Putting X25519 on that document would require deriving agreement from the identity key (forbidden) or fetching `sys/id` (not generative). Resolvers had no `keyAgreement` even after `sys/id` was published.

**Change.** Split the document. The generative document MUST NOT include `keyAgreement`. The operational document MUST include `keyAgreement` for `sys/id.agreement` when that record is available.

A generative document MUST grant `authentication` and `capabilityInvocation` to the 32-byte Ed25519 key and MUST NOT invent `keyAgreement`. Implementations MUST NOT convert the identity Ed25519 key to X25519 for Diffie–Hellman.

When a knot or client has `sys/id` (or equivalent hosted identity), `tangled.identity.resolve` and any `did:sol` document builder MUST return the operational document: the generative methods plus `keyAgreement` with id `did:sol:<base58>#agreement`, type `X25519KeyAgreementKey2020`, and `publicKeyBase58` of the X25519 `agreement` public key. Without `sys/id`, they MUST return the generative document.

See [Identifiers](./04-identifiers.md#did) and [Resolution](./13-resolution.md).

## 21.11 HTTP Message Signatures on ActivityPub import

**Problem.** ActivityPub imports required `attested: true` and matching well-known JSON `{did, actor}`. HTTP Message Signature headers on the foreign payload were not verified, so a mutated body could still be stored if the JSON proof matched.

**Change.** Keep the attestation rule. Verify RFC 9421 when signature fields are present.

Knots that ingest `sys.foreign` / `sys.binding` for `activitypub` MUST reject `attested: false`. Matching inline JSON `{did, actor}` remains REQUIRED when those fields are present. RFC 9421 is now required, not optional; see [21.16](#2116-activitypub-rfc-9421-required).

See [Interoperability](./16-interoperability.md#activitypub).

## 21.12 Signed Commit `prev` chain on `putRecord`

**Problem.** The reference knot advertised `getHead.commit` as the Merkle Search Tree (MST) root CID. Spec 07 already defined a signed Commit `{ did, version, data, prev, rev, sig }`. Import could not verify a commit chain.

**Change.** `putRecord` that advances a repo MUST produce a signed Commit. `getHead.commit` is that Commit CID. `data` on the head JSON is the MST root.

The Commit is DAG-CBOR signed with the Solana off-chain envelope (same construction as records). `data` MUST be the new MST root. `prev` MUST be the previous Commit CID, or absent/null at genesis. A client-supplied `prev` that does not equal the current head Commit CID MUST return `ConcurrentHeads` (HTTP 409). `rev` MUST increase; rollback MUST be refused. `getRepo` / import CAR MUST include Commit blocks. Import MUST verify each commit signature and the `prev` chain back to genesis or to `since`. Record-level `prev` (CRDT / Last-ts-wins) is unchanged.

See [Repositories](./07-repositories.md#commit) and [HTTP API](./12-http-api.md#tangled-repo-putrecord).

## 21.13 Compact `I(C)` retained and verified

**Problem.** Clients accepted a signature-valid compact plus uncovered ops and could not re-check a lying snapshot once `I(C)` blocks were gone.

**Change.** Compact records MUST permanently list `I(C)` as `included`. The authoring knot MUST NOT drop those blocks. `getRepo` / `getBlocks` MUST still serve them. A client MUST reject a compact if any CID in `I(C)` cannot be fetched and verified as a signed page op, or if Apply(`I(C)`) does not equal the snapshot. Stale concurrent ops still merge on top.

See [Data model](./06-data-model.md#compaction).

## 21.14 Subscribe `since` persisted

**Problem.** The subscribe event window lived only in memory and was lost on restart.

**Change.** An online knot MUST persist the commit event log next to the filesystem snapshot. Restart MUST still replay `since` for retained events.

See [Synchronization](./11-sync.md#live).

## 21.15 `getRepo ?since=` historical MST

**Problem.** A `since` CAR returned later commits plus the current public records, not the MST snapshots of intermediate commits.

**Change.** `since` MUST return the commit chain after `since` (and the `since` commit block) and the MST nodes plus records needed to reconstruct those commits. Import MUST verify the `prev` chain.

See [Synchronization](./11-sync.md#get-repo) and [Repositories](./07-repositories.md#car-export).

## 21.16 ActivityPub RFC 9421 required

**Problem.** Attested JSON `{did, actor}` without HTTP Message Signatures could still pass.

**Change.** ActivityPub `sys.foreign` / `sys.binding` ingest MUST have a verifiable RFC 9421 signature in addition to `attested: true` and `{did, actor}` when those fields apply. Attested-only MUST be rejected.

See [Interoperability](./16-interoperability.md#activitypub).

## 21.17 RFC 9421 covered-component set

**Problem.** The verified subset was only `@method` `@path` `content-digest`. Extra covered components were ignored.

**Change.** Implementations MUST implement `@method`, `@path`, `content-digest`, and MUST verify `@authority` (or `host`) and `content-type` when listed in `Signature-Input`. Other RFC 9421 features MAY be ignored.

See [Interoperability](./16-interoperability.md#activitypub).

## 21.18 `max_uses` persisted across restart

**Problem.** A forgotten `max_uses` counter reset to zero, so a one-use grant could be reused after restart.

**Change.** An online knot MUST persist the counter in the same snapshot as records. Restart MUST NOT reset counts.

See [Capabilities](./09-capabilities.md#caveats).

## 21.19 MLS wrapping vector vs live OpenMLS

**Problem.** `spec/vectors/mls-welcome-commit.json` `payload_hex` is wrapping-only; OpenMLS Welcome bytes are not byte-stable.

**Change.** That file remains the wrapping vector (ciphersuite, exporter label, record shape). Live RFC 9420 bytes are covered by `cargo test -p tangled --lib mls::`. OpenMLS Welcome hex is not frozen (not deterministic).

See [Test vectors](./vectors/README.md).

## 21.20 Public peer gossip on `node.describe`

**Problem.** A knot that knew one other knot had no specified way to learn that peer’s other public knots. There is no DHT.

**Change.** `tangled.node.describe` MAY include `peers`: an array of absolute knot base URLs the operator marked public.

A knot MUST advertise only its operator-configured public set (`--peer`, `TANGLED_PEERS`, or `{data}/peers.txt` in the reference knot). It MUST NOT re-advertise URLs learned from other knots. A connecting knot that successfully describes a peer MAY store that `peers` list and try those URLs for sync and resolve. Omitted `peers` means `[]`. Private or internal knots MUST NOT be listed.

See [HTTP API](./12-http-api.md#tangled-node-describe) and [Synchronization](./11-sync.md#peer-discovery).

## 21.21 Public node statistics

**Problem.** `node.describe` advertises profiles, blob limits, and public peers. Clients had no specified way to read knot-local public counts or hosted-identity topology without walking every `getHead` / `getRepo`.

**Change.** `GET /xrpc/tangled.node.stats` returns public-only aggregates (`counts`, retained public commit `rates`) and advertised-peer topology (`topology.peers`, `topology.hosted`).

A knot MUST count only live `vis: public` tips. It MUST NOT include `cap` / `sealed` records, capability or session counters, or learned peer URLs. `topology.peers` MUST equal `node.describe` `peers`. There is no global directory; clients MAY crawl advertised peers. See [HTTP API](./12-http-api.md#tangled-node-stats) and [Threat model](./18-threat-model.md#discovery-and-gateways).
