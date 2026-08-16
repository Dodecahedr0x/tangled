# How can a client build a home timeline and a profile feed from a Tangled knot?

## Answer
**Client-assembled.** No feed/timeline XRPC exists. Home: read own `tangled.graph.follow`, resolve each `subject` (optional `knot`), then `getHead`/`getRepo`/`subscribe` on that author’s knot; merge `pub.tangled.post`. Profile: `getRepo` that DID; filter `pub.tangled.post`. Wire enum is `getRepo` CAR (`since` = commit CID) or `getRecord`. Spec has no `listRecords`; `KnotClient::list_records` is an engine/test stand-in (knot fallback still serves it). `getRepo` is public-only; cap/sealed need `getRepoAuth`. No record-cursor. `queryBacklinks?limit=` 1–500 (default 50). Optional `loom.query`/`firehose` is overlay, not a home feed.

## Citations
- `spec/12-http-api.md:43-75,117-123` — describe, getRepo, no listRecords, queryBacklinks, loom
- `spec/http/openapi.yaml:10-11,140-236,426-445` — no feed/listRecords; getRepo `since`; subscribe; loom `q`/`type`
- `spec/11-sync.md:41-92`; `spec/08-visibility.md:16,57` — follow = pull; public getRepo; Auth for cap/sealed
- `spec/06-data-model.md:111-126`; `spec/20-use-cases.md:15-25` — follow + post + pull
- `crates/tangled/src/http.rs:350-566`; `knot.rs:417,503-531,1093` — client + engine listRecords; vis public
- `crates/tangled-knot/src/lib.rs:43-48` — fallback XRPC

## Confidence: high
Would drop if OpenAPI adds a feed method or listRecords becomes normative.

## Could not verify
- Engine `getRepo?since=` slices record CIDs (`knot.rs:1097-1100`), not a spec commit-CID MST diff; live CAR unconfirmed.
