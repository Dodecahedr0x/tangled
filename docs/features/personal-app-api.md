---
title: Personal browser app and API
slug: personal-app-api
description: Local browser launch, owner authentication, portable recovery and interoperable JSON workflows.
order: 18
---

# Personal browser app and API

Build or launch the complete local app with:

```sh
cargo run -p tangled-personal -- serve --data ./personal --listen 127.0.0.1:8788
```

The server opens the browser and prints `http://127.0.0.1:8788/#session=HEX`. `--no-open` prints the URL without launching a browser; port `0` chooses an available port. Only numeric loopback listen addresses are accepted. Use the exact printed address, including its port. The seven application assets and bundled Fira Sans font are embedded at build time: an installed binary works without its source checkout. Stop with Ctrl-C.

The fragment is a fresh random 256-bit owner credential for this process. It is never served in application HTML, discovery, or unauthenticated session metadata. The browser reads it, retains it in tab session storage and clears it from the visible URL. Keep the printed URL private. Another tab at the same exact local origin can request a connection from an already authenticated Personal tab using a same-origin browser channel. The credential remains in per-tab session storage; it is not stored in cookies or persistent local storage. Restart generates a new token; open the new printed URL. The server keeps each opened repository exclusively locked until it stops, and serializes workflow/storage operations on blocking workers.

## Using the app

- **Library:** search across collections, save searches, organize items and add annotations.
- **Capture:** save bookmarks, article text, screenshots or files; read and download originals.
- **Notes:** write notes and drafts, capture to your inbox, set reminder dates and attach files. Reminder dates are stored metadata; no background notifications are scheduled.
- **Reading:** subscribe to a feed URL, then import an RSS/Atom file. Readable article text, tags, stars and read state remain local. Subscription does not start background fetching.
- **Connections:** import a recovery `activity.json`, organize source items and contacts, then preview and explicitly approve a reply-intent download. Exporting a reply does not deliver it to the source service.
- **App access:** issue scoped expiring grants to a real app DID, download its certificate and revoke access. A certificate does not create an app's private key.
- **Recovery:** download an encrypted recovery file, verify it, synchronize a newer same-owner copy or explicitly restore to a fresh library. The app clears passphrase fields after each action.
- **Publish:** select individual records. Saved article text has a named inclusion control; original links and files require selection. Preview HTML and RSS, download them, or separately confirm a signed public publication. Hosting and delivery are separate actions.

Each view reads and writes the shared repository rather than a browser-only database.
Imported source markup is inert; feed reading reduces it to plain text. Generated
publication previews run in a sandbox. Advanced CID controls let third-party formats
choose known local blobs without recursively exposing nested private source data.

## HTTP contract

All data/session endpoints require `X-Tangled-Session: HEX`. This grants full owner authority, including recovery keys inside encrypted backup exports. Give it only to trusted local owner clients. There are no cookies or ambient credentials. Browser requests must have the exact matching `Origin`; command clients may omit `Origin`. All requests need the exact bound `Host`, including the port. Cross-site browser requests and CORS preflights from other origins are rejected. Responses use `Cache-Control: no-store`, MIME sniffing protection and a restrictive content security policy. This service has no TLS and must remain on loopback.

| Method and path | Request | Response |
| --- | --- | --- |
| `GET /api/v1/manifest` | Public | Version, authentication description, limits, feature actions and collection schema links |
| `GET /api/v1/docs` | Public | This API document as Markdown |
| `GET /api/v1/schemas/{collection}` | Public | Embedded open collection JSON schema |
| `GET /api/v1/session` | Authenticated | `{did,data,version,apiVersion,features}` |
| `POST /api/v1/personal/{feature}` | Authenticated JSON workflow input | Existing `tangled_personal::execute` JSON result |
| `POST /api/v1/backups/export` | `{passphrase}` | `{filename,backup}` encrypted recovery packet |
| `POST /api/v1/backups/verify` | `{passphrase,backup}` | Verified continuity status |
| `POST /api/v1/backups/sync` | `{passphrase,backup}` | Same-owner causal sync status |
| `POST /api/v1/backups/restore` | `{passphrase,backup,label}` | `{restored,session}` for a fresh restored repository |

POST requests require `Content-Type: application/json` and a JSON object. Workflow bodies are limited to 64 MiB; recovery wrapper bodies to 128 MiB + 64 KiB (the packet hex-encodes ciphertext). Existing per-feature limits also apply. Errors are non-2xx JSON `{error:string}`: authentication `401`, Host/Origin `403`, workflow/JSON validation `400`, unknown route `404`, wrong method `405`, body size `413`, media type `415`, worker failure `500`. The API performs the real signed persistent Store mutations used by the CLI; it has no separate application database. API v1 is not raw Knot XRPC, and exposes no unguarded engine import/blob routes.

Supported features: `info`, `records`, `memory`, `capture`, `notes`, `feeds`, `agents`, `devices`, `publish`, `connections`. Workflow action fields and result envelopes are described by the adjacent `personal-*.md` documents and listed in discovery. Unknown extension fields remain governed by those existing workflows. HTTP `devices` supports `status`; recovery file paths are replaced by the packet routes above. HTTP publishing returns downloadable `files` and rejects CLI-only `outputDir`. Caller-provided filesystem paths never select browser recovery inputs or outputs.

## Another owner client

Take `HEX` from the owner startup URL and send it in the header, never a query parameter. For example, these calls are equivalent to creating and retrieving a note in the browser:

```sh
curl --fail-with-body http://127.0.0.1:8788/api/v1/personal/notes \
  -H 'X-Tangled-Session: HEX' -H 'Content-Type: application/json' \
  --data '{"action":"create","rkey":"example","body":{"title":"From another client","text":"Portable signed content"}}'
curl --fail-with-body http://127.0.0.1:8788/api/v1/personal/records \
  -H 'X-Tangled-Session: HEX' -H 'Content-Type: application/json' \
  --data '{"action":"get","collection":"pub.tangled.note","rkey":"example"}'
```

Generic records support `list` (optional `collection`), `get` (`collection`,`rkey`), `put` (`collection`,`rkey`,`body`, optional `visibility`, default `cap`) and `delete` (`collection`,`rkey`). Results contain normal signed envelopes, exact CIDs and stable `tangled://` URIs. An independent client can discover schemas, read records and write its own compatible extensions. Private records are readable by this local custodial owner process; `cap` is not protection from the operator.

## Signed agent invocation

The owner uses `POST /api/v1/personal/agents` with `{"action":"grant","audience":"did:sol:AGENT_PUBLIC_KEY","collection":"pub.tangled.note","access":"read","ttl_seconds":3600}`. The returned `capability` is a signed, scoped certificate. For `search` or `write`, sign a DAG-CBOR Invocation with the agent's own private key, bind its arguments to the exact JSON input without `invocation`, and supply its hex bytes as `invocation`:

```rust,ignore
use tangled::capability::{Capability, Invocation};
let capability = Capability::from_json(&grant["capability"])?;
let mut arguments = serde_json::json!({
    "action":"search", "collection":"pub.tangled.note", "query":"orchid"
});
let resource = format!("tangled://{owner_did}/pub.tangled.note");
let mut invocation = Invocation::new(
    &capability, &capability.cmd, &resource, &owner_did,
    unix_seconds, fresh_random_nonce_16_bytes,
)?;
invocation.args = arguments.clone();
invocation.sign(agent_identity.signing_key())?;
arguments["invocation"] = serde_json::json!(hex::encode(invocation.encode_cbor()?));
// Trusted owner client POSTs arguments to /api/v1/personal/agents.
```

The service checks certificate signatures, audience, scope, signed arguments, expiry, revocation and replay, then retains signed activity receipts and URI/CID citations. See `personal-agents.md` for write, revoke and receipt details. The HTTP owner token is still required and is **not** a scoped agent credential. An untrusted external agent must use a trusted owner-side broker that forwards only the signed `search` and `write` inputs; never give it this full-owner session token. No new external model, connector credential, live remote fetch or message-send integration is implied.

## Browser recovery and restart

Export downloads a JSON object containing the existing authenticated encrypted recovery packet (`tangled.personal.recovery.v1`); the response wrapper supplies its filename. Browser code downloads `backup` as the file contents. Verification, sync and restore upload that packet object with its passphrase, without server path entry. The passphrase must be a separate 12–1024 byte secret. Backups contain full owner signing/agreement keys and attachments; keep the file and passphrase safe.

Sync only accepts the same owner and a valid causal successor; stale or divergent transfers fail without replacing the active library. Restore accepts a label of 1–80 ASCII letters, numbers, spaces, hyphens or underscores. It creates a random fresh private directory at `ORIGINAL_DATA/restored/LABEL-RANDOM`; an existing directory is never overwritten. Only after successful authenticated restore and opening the new repository does the running app switch its current Store. Failed authentication leaves the current Store unchanged. Previous stores stay locked for the remainder of the server lifetime. Each process permits at most four restores to bound retained engines; restart with the current library path before restoring again.

The restore result and subsequent session metadata expose the absolute restored location in `session.data`. The owner interface must surface it. **To continue using the restored library after stopping the server, launch with `--data` set to that location.** Launching with the original directory opens the original library. This explicit choice avoids silently redirecting other independent local clients. Recovery staging uses private temporary directories containing encrypted packet bytes and removes them when the operation ends; plaintext recovered owner material stays within the existing recovery implementation.
