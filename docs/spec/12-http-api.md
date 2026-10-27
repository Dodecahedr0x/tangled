---
title: HTTP API
slug: http-api
order: 12
group: wire
---

# HTTP API

Knots speak HTTP. Method names follow `tangled.<ns>.<name>`. The path is `/xrpc/<method>`. GET for cacheable reads, POST for bodies and invocations that do not fit a query string.

Machine-readable: [`docs/spec/http/openapi.yaml`](./http/openapi.yaml).

## Content negotiation

Clients MUST send `Accept: application/cbor, application/json` (cbor first). Knots MUST serve `application/cbor` (DAG-CBOR) when `Accept` prefers `application/cbor`, and MAY serve `application/json` using the IPLD JSON mapping. `Content-Type` on POST MUST be one of those two except `putBlob` (`application/octet-stream`) and `import` (`application/vnd.ipld.car`).

CIDs in query strings are multibase base32 (`b…`).

## Errors

JSON/CBOR map:

```
{ "error": "CapabilityRequired", "message": "…", "retry": false }
```

| error | HTTP | When |
| --- | --- | --- |
| `InvalidRequest` | 400 | Bad params |
| `InvalidSignature` | 400 | Sig verify failed |
| `CapabilityRequired` | 401 | Missing cap |
| `CapabilityInvalid` | 403 | Cap present but fails |
| `NotFound` | 404 | No such DID/record |
| `MethodNotImplemented` | 404 | Unknown `tangled.*` method on a Tangled knot |
| `ProfileRequired` | 501 | Method needs a profile the knot omitted |
| `BlobTooLarge` | 413 | |
| `RateLimited` | 429 | Including page compact admission (`PAGE_COMPACT_MIN_OPS`, `PAGE_COMPACT_MIN_INTERVAL_MS`). MUST include `Retry-After` as delta-seconds (RFC 9110). For compact admission the value MUST be at least `PAGE_COMPACT_MIN_INTERVAL_MS / 1000` (300). |
| `Unavailable` | 503 | |
| `ConcurrentHeads` | 409 | Conflict-free Replicated Data Type (CRDT) `putRecord` would orphan the current Merkle Search Tree (MST) value; body includes `head` |

## Methods

### tangled.node.describe

`GET`. No auth. Returns profiles, protocols, maxBlob, knot DID, and `peers`.

`peers` is an array of absolute knot base URLs this operator marked public (`https://knot.example`). Empty when the operator listed none. A knot MUST advertise only that configured public set. It MUST NOT re-advertise URLs it learned from other knots. Connecting knots and clients MAY remember `peers` for their own `describe` / `getHead` / `identity.resolve`; they MUST NOT treat the list as trusted topology. Private or internal knots MUST NOT appear in `peers`.

### tangled.node.stats

`GET`. No auth. Returns knot-local **public** aggregates and advertised-peer topology.

```json
{
  "v": 1,
  "did": "did:sol:…",
  "asOf": 1700000000,
  "counts": {
    "identities": 1,
    "repos": 1,
    "records": 2,
    "blobs": 0,
    "peers": 1,
    "circles": 0,
    "foreign": 0,
    "commits": 2
  },
  "rates": {
    "publicCommits": 2,
    "retained": 256
  },
  "topology": {
    "peers": ["https://b.example"],
    "hosted": [{ "did": "did:sol:…", "handle": "alice.sol", "rev": "3jzfcijpjiz2u", "records": 2 }],
    "truncated": false
  }
}
```

Counts MUST include only live `vis: public` record tips (`body` not a tombstone). They MUST NOT include `vis: cap` or `vis: sealed` records, capability-use counters, SIWS sessions, or any other private activity. `blobs` is the number of distinct blob CIDs referenced by those public tips. `commits` and `rates.publicCommits` are the length of the retained public subscribe log (this knot’s recent public commits, capped at `rates.retained`). They are not a wall-clock hourly rate.

`topology.peers` MUST be the same operator-configured public set as `node.describe` `peers`. A knot MUST NOT list learned peer URLs. `topology.hosted` lists Decentralized Identifiers (DIDs) that have at least one live public tip on this knot (the same DIDs `getHead` / `getRepo` already expose). The list MAY be truncated; when it is, `truncated` MUST be `true`. `handle` is the first non-DID `alsoKnownAs` from `sys.id` when present.

There is no global directory. Clients assemble a network view by calling `node.stats` (and `node.describe`) on a configured knot, then on that knot’s advertised `peers`. They MUST NOT treat the graph as complete or trusted.

### tangled.sync.getHead

`GET ?did=`. Returns `{ did, commit, rev, knot, data }`. `commit` is the signed [Commit](./07-repositories.md#commit) CID. `data` is that commit’s MST root CID. See [Synchronization](./11-sync.md).

### tangled.sync.getRecord

`GET ?did=&collection=&rkey=`. Optional `Authorization: Cap <base64url(DAG-CBOR)>`. JSON-in-header MUST be rejected.

### tangled.sync.getBlob

`GET ?did=&cid=`. Raw blob bytes. `Content-Type` MUST be `application/octet-stream`. The response MUST include `Content-Length` equal to the byte length. When the writer stored a media type (`putBlob` `mediaType` query or `X-Tangled-Media-Type`), the knot MUST also send `X-Tangled-Media-Type`. No capability when a live public record references `cid`; otherwise `Authorization: Cap` (`blob/get`, `repo/read`, or `circle/read`). See [Synchronization](./11-sync.md#get-blob).

### tangled.sync.getBlocks

`POST` `{ "cids": ["b…"] }`.

### tangled.sync.getRepo

`GET ?did=&since=&blobs=`. Returns a CAR of the **public** repository. There is no `tangled.sync.listRecords` method on the wire; clients MUST use `getRepo` (or `getRecord`) to enumerate. An in-process engine MAY offer `listRecords` as a test convenience; a knot MUST NOT require clients to call it.

Without `since`, the full public repo (commits, public MST snapshots, public records). With `since` (a commit CID), the commit chain after `since` through head, the `since` commit block, and the MST nodes plus records needed to reconstruct those commits. Blobs are not included unless `blobs=true`. For `pub.tangled.page`, the CAR MUST include `I(C)` blocks of each compact ([Data model](./06-data-model.md#compaction)). It MUST still include the compact record. Import MUST verify the `prev` chain.

### tangled.sync.getRepoAuth

`POST` with `Authorization: Cap <base64url(DAG-CBOR(Invocation))>` and JSON/CBOR body `{ did, since, blobs }`. `since` MUST be honored as a query parameter and, when the query omits it, from the body. If both are present and differ, the query parameter MUST win. For `circle/read`, the CAR is only circle-scoped records (and the circle log on the owner). Sibling collections MUST NOT be included.

### tangled.sync.queryBacklinks

`GET ?cid=&collection=&limit=`. Local public backlink index. Knots SHOULD implement it; they MUST NOT crawl. See [Synchronization](./11-sync.md#query-backlinks).

### tangled.sync.notify

`POST` `{ did, commit }`.

### tangled.sync.subscribe

`GET` WebSocket. Optional `since` (commit CID) resumes after that commit; see [Synchronization](./11-sync.md#live).

### tangled.repo.putRecord

`POST` `{ record }` plus one of: a Sign In With Solana (SIWS) session cookie for `author`, a `repo/write` capability, or a hosted identity-key write. The knot MUST verify the record’s identity-key (or device-key) signature and MUST NOT accept a record whose `author` the caller cannot speak for. SIWS MUST NOT replace that signature.

A **hosted identity-key write** is a `putRecord` whose `author` is a DID this knot hosts, the record has no `signedBy`, and the signature verifies with that DID’s identity key. The knot MAY accept it without a SIWS cookie (CLI / self-hosted). Device-key writes (`signedBy` present) MUST present `repo/write` or a SIWS session for `author`. After `sys.revocation` of a device capability, subsequent writes that present that capability MUST fail `CapabilityInvalid`.

A `putRecord` that advances the repository MUST produce a new signed Commit whose `data` is the new MST root and whose `prev` is the previous Commit CID (absent at genesis). The request MAY include `prev` (a Commit CID). When the DID already has a head, a supplied `prev` that does not equal the current head Commit CID MUST return `ConcurrentHeads` (HTTP 409) with `head` set to that Commit CID. When there is no head, a supplied `prev` MUST return `InvalidRequest`. Omitted `prev` means the current head (or genesis). `rev` MUST increase; a rollback MUST be refused.

For CRDT collections, a write that would orphan the current MST value MUST return `ConcurrentHeads` ([Data model](./06-data-model.md#page-crdt)). A page compact that fails `PAGE_COMPACT_MIN_OPS` or `PAGE_COMPACT_MIN_INTERVAL_MS` MUST return `RateLimited` ([Data model](./06-data-model.md#compaction)).

### tangled.repo.putBlob

`POST` raw bytes, `Content-Type: application/octet-stream`. Optional `mediaType` query parameter or `X-Tangled-Media-Type` request header (a Multipurpose Internet Mail Extensions (MIME) type). The knot MUST store that type with the blob when present. Returns `{ cid }`.

### tangled.repo.import

`POST` a CAR (`Content-Type: application/vnd.ipld.car`) plus a SIWS session for the imported DID **or** a `repo/write` capability on that DID. The knot MUST verify every commit signature in the CAR and MUST verify the `prev` chain from the CAR root back to genesis (or to `since` when the export was a diff). It MUST refuse a DID it already hosts unless `force` is true (recovery).

### tangled.identity.resolve

`GET ?handle=` or `?did=`. Returns DID, `sys/id` CID, knot endpoints, and `document` (the generative or operational DID document; see [Identifiers](./04-identifiers.md#did)). The knot MAY resolve handles it does not host by performing DNS/SNS lookup as specified in [Resolution](./13-resolution.md).

### tangled.session.challenge

`GET ?did=`. No cookie required. Returns the canonical SIWS message the caller MUST sign. See [Sessions (SIWS)](#sessions-siws).

### tangled.session.verify

`POST` `{ did, nonce, signature }`. `signature` is hex-encoded Ed25519 over the UTF-8 SIWS message. On success the knot MUST set cookie `tangled_session`. See [Sessions (SIWS)](#sessions-siws).

### tangled.cap.verify

`POST` a DAG-CBOR Invocation (same encoding as the Cap header). JSON MUST be rejected. Returns `{ ok: true, cmd, res }` or `CapabilityInvalid`. Useful for gateways.

### tangled.loom.query

`GET ?q=&type=`. OPTIONAL. Looms only. When `type=ref`, `q` is a record CID. Response includes `hits`, untrusted `count`, `overlay: true`, `complete: false`. See [Looms](./14-looms.md).

### tangled.loom.firehose

`GET` WebSocket of public commits. OPTIONAL. Looms only.

## Sessions (SIWS)

Sign In With Solana authenticates a browser to **this knot** for writes as the identity. It is knot-local. SIWS MUST NOT replace record signatures: `putRecord` is still signed with the off-chain envelope.

### Message

The knot advertises a **domain** equal to the host of its public URL (no scheme, no path, no port; `localhost` when the URL is unset). The canonical message MUST be exactly these five UTF-8 lines (`\n` separators, no trailing blank line):

```
tangled.siws.v1
domain: <domain>
did: <did>
nonce: <nonce>
issuedAt: <unix-seconds>
```

`nonce` MUST be 16 cryptographically random bytes, hex-encoded (32 hex characters). `issuedAt` MUST be Unix seconds. Challenges MUST expire; 300 seconds is RECOMMENDED.

The signature MUST be Ed25519 over the UTF-8 bytes of that message, using the identity key of `did`. Implementations MUST NOT wrap the SIWS message in the Solana off-chain envelope ([Cryptography](./05-cryptography.md#sign-in-with-solana)).

### Cookie

Successful `tangled.session.verify` MUST set:

```
Set-Cookie: tangled_session=<token>; HttpOnly; Path=/; Max-Age=86400; SameSite=Lax
```

The cookie MUST also include `Secure` when the knot’s advertised public URL is `https://`. Token MUST be 32 cryptographically random bytes, hex-encoded. Sessions MUST expire after 86400 seconds (`Max-Age`). Cookie name MUST be `tangled_session`.

### Who may write without a cookie

Hosted identity-key writes (no `signedBy`) MAY succeed without this cookie, as specified under `putRecord`. Import MAY use SIWS **or** `repo/write`.

A knot MAY hold a delegated device key and sign on behalf of a web client only if the user issued a `repo/write` capability to that device key; that is an implementation choice and MUST be disclosed in `node.describe` as `custodialSign: true`. Default is `false`.

## CORS

Public unauthenticated reads MUST send `Access-Control-Allow-Origin: *`.

When the request includes `Cookie` or `Authorization`, the response MUST NOT use `*`. It MUST echo the request `Origin`, MUST set `Access-Control-Allow-Credentials: true`, and SHOULD set `Vary: Origin`.

`OPTIONS` preflight MUST use the same credentialed rule, keyed off `Access-Control-Request-Headers`. When that header lists `Cookie` or `Authorization` (comma-separated, case-insensitive), the preflight MUST NOT use `*`. It MUST echo `Origin`, MUST set `Access-Control-Allow-Credentials: true`, and SHOULD set `Vary: Origin`. If that preflight has no `Origin`, the knot MUST omit `Access-Control-Allow-Origin`. Otherwise `OPTIONS` MUST send `Access-Control-Allow-Origin: *`.

Knots MUST allow request headers `Authorization`, `Content-Type`, `Accept`, and `Cookie`, and methods `GET`, `POST`, `OPTIONS`.

## Transport

Production knots MUST use HTTPS. `localhost` MAY use HTTP. HTTP/2 is RECOMMENDED.
