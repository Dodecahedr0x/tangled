---
title: HTTP API
slug: http-api
order: 12
---

# HTTP API

Knots speak HTTP. Method names follow `tangled.<ns>.<name>`. The path is `/xrpc/<method>`. GET for cacheable reads, POST for bodies and invocations that do not fit a query string.

Machine-readable: [`spec/http/openapi.yaml`](./http/openapi.yaml).

## Content negotiation

Clients MUST send `Accept: application/cbor, application/json`. Knots MUST serve `application/cbor` (DAG-CBOR) when requested, and MAY serve `application/json` using the IPLD JSON mapping. `Content-Type` on POST MUST be one of those two.

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
| `RateLimited` | 429 | |
| `Unavailable` | 503 | |

## Methods

### tangled.node.describe

`GET`. No auth. Returns profiles, protocols, maxBlob, knot DID.

### tangled.sync.getHead

`GET ?did=`. See [Synchronization](./11-sync.md).

### tangled.sync.getRecord

`GET ?did=&collection=&rkey=`. Optional `Authorization: Cap`.

### tangled.sync.getBlob

`GET ?did=&cid=`. Raw blob bytes. No capability when a live public record references `cid`; otherwise `Authorization: Cap` (`blob/get`, `repo/read`, or `circle/read`). See [Synchronization](./11-sync.md#get-blob).

### tangled.sync.getBlocks

`POST` `{ "cids": ["b…"] }`.

### tangled.sync.getRepo

`GET ?did=&since=&blobs=`.

### tangled.sync.getRepoAuth

`POST` invocation + `{ did, since, blobs }`. For `circle/read`, the CAR is only circle-scoped records (and the circle log on the owner).

### tangled.sync.queryBacklinks

`GET ?cid=&collection=&limit=`. Local public backlink index. Knots SHOULD implement it; they MUST NOT crawl. See [Synchronization](./11-sync.md#query-backlinks).

### tangled.sync.notify

`POST` `{ did, commit }`.

### tangled.sync.subscribe

`GET` WebSocket.

### tangled.repo.putRecord

`POST` `{ record }` plus either identity session (SIWS cookie, knot-local) or `repo/write` capability. The knot MUST verify the record signature and MUST NOT accept a record whose `author` the caller cannot speak for.

### tangled.repo.putBlob

`POST` raw bytes, `Content-Type: application/octet-stream`. Returns `{ cid }`.

### tangled.repo.import

`POST` a CAR (`Content-Type: application/vnd.ipld.car`) plus identity session or `repo/write` on the DID. The knot MUST verify every commit signature in the CAR and MUST refuse a DID it already hosts unless `force` is true (recovery).

### tangled.identity.resolve

`GET ?handle=` or `?did=`. Returns DID, `sys/id` CID, knot endpoints. The knot MAY resolve handles it does not host by performing DNS/SNS lookup.

### tangled.cap.verify

`POST` an Invocation. Returns `{ ok: true, cmd, res }` or `CapabilityInvalid`. Useful for gateways.

### tangled.loom.query

`GET ?q=&type=`. OPTIONAL. Looms only. When `type=ref`, `q` is a record CID. Response includes `hits`, untrusted `count`, `overlay: true`, `complete: false`. See [Looms](./14-looms.md).

### tangled.loom.firehose

`GET` WebSocket of public commits. OPTIONAL. Looms only.

## Sessions (SIWS)

Sign In With Solana authenticates a browser to **this knot** for `putRecord` as the identity. The SIWS message MUST include the knot domain, the DID, a nonce, and `issuedAt`. Sessions MUST expire (RECOMMENDED 24 hours). SIWS MUST NOT replace record signatures: the record is still signed with the off-chain envelope. A knot MAY hold a delegated device key and sign on behalf of a web client only if the user issued a `repo/write` capability to that device key; that is an implementation choice and MUST be disclosed in `node.describe` as `custodialSign: true`. Default is `false`.

## CORS

Public reads MUST be CORS-available (`Access-Control-Allow-Origin: *`). Authorized methods MUST NOT use `*`; they MUST echo a specific origin or use a non-browser client.

## Transport

Production knots MUST use HTTPS. `localhost` MAY use HTTP. HTTP/2 is RECOMMENDED.
