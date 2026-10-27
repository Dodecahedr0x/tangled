---
title: Personal agent workspace
slug: personal-agents
description: Issue signed scoped grants and retain cited agent results and receipts.
order: 14
---

# Personal agent workspace

This local owner-side service gives an external agent a real Tangled capability for a single collection. The agent creates and keeps its own identity key; grants never contain the owner's key. No model integration or purchased API is needed. Records, grants, revocations, and activity receipts are signed Tangled records. Private visibility means capability-gated access, not encryption from the local operator.

Run owner controls locally:

```sh
cargo run -p tangled-personal -- --data ./personal agents < input.json
```

Each command reads one JSON object. Owner-only operations assume trusted local access to the data directory; this CLI is not a network server or a sandbox for untrusted code. Give an external agent its certificate and expose only the `search` and `write` operations through a trusted caller. Do not give it the data directory or a shell running as the owner.

## Owner controls

- `grant`: `{"action":"grant","audience":"did:sol:AGENT_PUBLIC_KEY","collection":"pub.tangled.note","access":"read","ttl_seconds":3600}`. Use the agent's actual DID. Access is `read` or `write`; TTL is 1–86400 seconds. Each grant is one collection and one command (`repo/read` or `repo/write`), with a distinct signed certificate CID. Root and wildcard scopes, system collections, and workspace control collections cannot be granted. Issue separate grants for separate permissions.
- `revoke`: `{"action":"revoke","cid":"b...","reason":"task finished"}`. Returns a signed `sys.revocation` record with the protocol rkey (capability CID without its multibase prefix). Revocation survives restart and blocks future actions.
- `list`: `{"action":"list"}`. Returns stored grant envelopes with certificate bodies, stable URIs, and exact CIDs. This is issuance history, including expired and revoked certificates; use certificate expiry and `sys.revocation` to determine current authority.
- `report`: `{"action":"report"}`. Returns signed activity receipts for successful delegated searches and writes. Each includes the agent DID, capability CID, invocation CID, signed request arguments, and URI/CID citations for all returned records or the written result.

The grant response includes `capability` (signed certificate as JSON, with hex signature), `cid` (its protocol DAG-CBOR CID), and `record` (the signed private grant envelope). The JSON representation is for transport; signature verification uses protocol DAG-CBOR and the actual issuer public key.

## Agent requests

Search arguments: `{"action":"search","collection":"pub.tangled.note","query":"orchid"}`. Searches current nondeleted record bodies by case-insensitive substring and returns at most 100 matches in URI order, plus an activity receipt. Query length is limited to 4096 bytes. It cannot enumerate sibling collections, attachments, grant records, or activity records.

Write arguments: `{"action":"write","collection":"pub.tangled.note","rkey":"agent-result","body":{"summary":"Orchid observations","sources":[{"uri":"tangled://did:sol:OWNER/pub.tangled.note/source","cid":"b..."}]}}`. Requires a write certificate; the body must be an object, at most 256 KiB. Creates a new private result record under the owner identity through the verified local service. Caller-supplied source citations are claims; the receipt's output citation identifies the exact result actually stored. Existing record keys, including tombstoned keys, are rejected, preserving owner data and unknown extensions. Writing does not grant reading existing private data. Receipts contain the signed request, so writes should contain only material the owner expects in its private activity history.

For either operation, add an `invocation` string containing hex-encoded signed DAG-CBOR. The invocation's `args` must exactly equal the JSON input with `invocation` removed. The agent signs it with the private key corresponding to the grant audience:

```rust,ignore
use tangled::capability::{Capability, Invocation};
let cap = Capability::from_json(&grant_response["capability"])?;
let resource = format!("tangled://{owner_did}/pub.tangled.note");
let mut invocation = Invocation::new(
    &cap, &cap.cmd, &resource, &owner_did, unix_seconds, fresh_nonce_16_bytes,
)?;
invocation.args = arguments.clone();
invocation.sign(agent_identity.signing_key())?;
arguments["invocation"] = serde_json::json!(hex::encode(invocation.encode_cbor()?));
```

Use a fresh 16-byte nonce for every request. The service verifies owner certificate and agent invocation signatures, issuer, audience, exact collection, command, CID, signed arguments, issuance history, time, and persisted revocations before accessing records. Personal grants expire exactly at `exp`; the protocol clock skew allowance does not extend this local workspace's TTL. Invocation timestamps permit 60 seconds future skew and at most 300 seconds age. Successful nonce reuse is rejected across restarts, including after its activity receipt is tombstoned. Only direct owner-issued certificates are supported; chained redelegation is rejected.

The service signs result envelopes and receipts as owner. It does not give agents root credentials or claim independently agent-authored result records. Each receipt retains the signed invocation as hex DAG-CBOR in `invocation_cbor`, alongside its DAG-CBOR CID, so another client can verify agent authorship independently. An interrupted process between storing a result and storing its receipt can leave the result without a receipt; writes and receipts are separate signed repository operations. Local file permissions and the Store's exclusive lock protect the custodial owner boundary.

## Extensible records

- `pub.tangled.agent.grant`: `cid` plus `capability` certificate. Schema: `docs/spec/schemas/pub.tangled.agent.grant.json`.
- `pub.tangled.agent.activity`: `actor`, `action`, `capability`, `invocation`, `invocation_cbor`, `request`, `citations`. Schema: `docs/spec/schemas/pub.tangled.agent.activity.json`.

Both schemas permit unknown extensions. They add optional application-independent collections and do not change normative v1 capability or revocation schemas. Any Tangled client can inspect the signed envelopes and protocol certificates.
