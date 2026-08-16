# How does a Tangled client talk to a real (not mock) knot over HTTP?

## Answer
Path `/xrpc/tangled.<ns>.<name>` on the knot origin. OpenAPI server is placeholder `https://knot.example`. Production MUST HTTPS; localhost MAY HTTP.

Messaging: `GET node.describe`; `GET sync.getRecord?did&collection&rkey` (optional Cap); `GET sync.getRepo?did&since&blobs` → CAR; `POST sync.getRepoAuth` (Cap); `GET sync.getBlob` / `POST repo.putBlob`; `POST repo.putRecord {record}` (SIWS cookie or `repo/write` Cap); `GET sync.subscribe` / `GET loom.firehose` are **101 WebSocket**.

`KnotClient::http(base)`: reqwest `rustls-tls`, trim `/`. `with_cap_header` sets `Authorization` every call. `cap_authorization_header` → `Cap <base64url(DAG-CBOR Invocation)>`. No cookie jar — native apps use Cap.

No public knot documented; URL is out-of-band. `tangled-knot` is local HTTP (`127.0.0.1:3000`). Client subscribe/firehose are plain GET (no 101); MockKnot errors; knot crate **does** WS. `list_records` is mock-only. Poll `get_head`/`get_repo`.

App fetches TXT `_tangled.<h>` and/or `https://<h>/.well-known/tangled-did`, then `verify_handle` vs `sys/id.alsoKnownAs`. DID→knot: `sys/id` endpoints, optional on-chain `TangledKnot`, else OOB.

## Citations
- `spec/http/openapi.yaml:8-273,357-369`; `spec/12-http-api.md:9-121`; `spec/11-sync.md:55-59` — methods, Cap, SIWS, TLS, WS
- `spec/13-resolution.md:11-42`; `spec/04-identifiers.md:51-60`; `spec/09-capabilities.md:121` — DNS/well-known, Cap header
- `spec/00-abstract.md:26`; `README.md:5` — not a live network
- `crates/tangled/src/http.rs:10,266-547,575-620`; `capability.rs:643-646`; `handle.rs:6-36`; `knot.rs:222-224`; `Cargo.toml:30`; `README.md:104-106` — construct, Cap, no DNS I/O, GET/POST, rustls, MockKnot WS unimplemented
- `crates/tangled-knot/src/main.rs:15-27`; `lib.rs:43-46`; `ws.rs:1-23` — local HTTP knot + server WS

## Confidence: high
Would drop if `KnotClient::http` gains a real WS/cookie client; claims are cited from this tree.

## Could not verify
- Public knot on the internet: spec/site claim none; web not searched per instructions.
- `tangled-knot` TLS: CLI binds HTTP; no TLS in that crate.
- Whether reqwest 0.12 follows well-known same-host redirects (spec requires same-host only).
