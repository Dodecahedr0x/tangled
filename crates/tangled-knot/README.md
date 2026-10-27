# tangled-knot

HTTP knot: a PDS-like host for a signed MST repo. Serves the Tangled OpenAPI (`docs/spec/http/openapi.yaml`).

Protocol logic (CID, signatures, MST, vis, capabilities, HPKE, MLS, page CRDT, CAR, foreign_id, SIWS, handle resolve) lives in [`tangled`](../tangled/). This crate is the axum binding, a filesystem snapshot of the engine, and WebSocket `subscribe` / `loom.firehose`.

## Private social archives

Persistent self-hosted knots with a local `identity.json` expose the authenticated archive at `/imports`. Use the local identity’s SIWS session. Archives, original files, and media are owner-only; live helpers require a separate source/account-scoped connection token. See [social imports](../../docs/features/social-imports.md) and [connector setup](../../docs/features/connectors.md).

## Run

From the workspace root:

```bash
cargo run -p tangled-knot -- --listen 127.0.0.1:3000 --data ./data
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `--listen ADDR` | `127.0.0.1:3000` | Bind address. Use `:0` for an ephemeral port. |
| `--data DIR` | `./data` | Identity seeds, records, blobs. |
| `--public-url URL` | `http://<bound>` | Advertised in `getHead.knot` and `identity.resolve`. |
| `--peer URL` | none | Public peer knot URL in `node.describe` `peers` (repeatable). |
| `TANGLED_PEERS` | unset | Extra public peer URLs (comma or space). |
| `{data}/peers.txt` | absent | One public peer URL per line (`#` comments). |
| `SOLANA_RPC` | unset | JSON-RPC for SNS `.sol` handles. Default tests never set this; CI uses a local validator. |

The process prints the knot DID and the bound address. Identity is created at `{data}/identity.json` on first run.

`localhost` HTTP is allowed (spec 12). Production knots must use HTTPS in front of this process (`--public-url https://…` sets the SIWS cookie `Secure` flag). Handle resolution uses live DNS-over-HTTPS plus well-known HTTPS. SNS uses `SOLANA_RPC` when set.

## Complete vs opaque

| Area | Status |
| --- | --- |
| `node.describe`, `node.stats`, `getHead`, `getRecord`, `putRecord` | Complete |
| `listRecords` | `MethodNotImplemented` on the wire (engine helper only) |
| vis `public` / `cap` / `sealed`; `repo/read` `repo/write` `circle/read` | Complete |
| Cap invocations `Authorization: Cap <base64url(DAG-CBOR)>`; JSON 403 | Complete |
| Attenuation, expiry, revocation (`sys.revocation`), sibling deny | Complete |
| Device-revoke enforcement on subsequent writes | Complete |
| `putBlob` / `getBlob` (`getblob_auth`; 413 `BlobTooLarge`; `Content-Length` + `X-Tangled-Media-Type`) | Complete |
| `getRepo` / `getRepoAuth` / `getBlocks` / `import` CARv1; `since` query or body | Complete |
| `max_uses` counted per leaf cap CID; overflow 403; persisted across restart | Complete |
| Credentialed CORS preflight (echo Origin + Allow-Credentials) | Complete |
| `RateLimited` 429 includes `Retry-After` | Complete |
| Import requires SIWS session or `repo/write` | Complete |
| Page compact admission; last-ts-wins MUST NOT compact | Complete |
| ConcurrentHeads 409; signed Commit `prev` on `putRecord`; `getHead.commit` is the Commit CID | Complete |
| `queryBacklinks` / `loom.query` overlay (`complete: false`) | Complete |
| Circles: store/serve `sys.circle` and MLS Welcome/Commit records | Complete |
| `identity.resolve` (hosted DID + DNS TXT / well-known / SNS; generative vs operational document) | Complete |
| SIWS `session.challenge` / `session.verify`; cookie `tangled_session` (`Secure` on https) | Complete |
| `sync.subscribe` / `loom.firehose` WebSocket | Complete (public commits; cap on non-public subscribe; `since` replay persisted) |
| ActivityPub `sys.foreign` MUST be `attested: true` + matching `{did, actor}` + verifiable RFC 9421 | Complete |
| ActivityPub S2S gateway (WebFinger, actor, inbox, outbox) | Complete (same process; `crates/tangled-ap`) |
| DAG-CBOR when `Accept` prefers `application/cbor` | Complete |
| Gateway product / global firehose / app UI | Out of scope |

## ActivityPub (Mastodon S2S)

The knot speaks **server-to-server ActivityPub** in-process (not a sidecar). Reed stays a Tangled client: follow `acct:user@instance` via `GET /xrpc/tangled.ap.resolve`, then `tangled.graph.follow`. No Mastodon REST or OAuth.

| Route | Role |
| --- | --- |
| `GET /.well-known/webfinger?resource=acct:…` | JRD → actor |
| `GET /users/{name}` | Person + HTTP-sig public key |
| `POST /inbox` and `POST /users/{name}/inbox` | Verify draft-cavage `rsa-sha256`, ingest |
| `GET /users/{name}/outbox` | Public posts as `Note`s |
| `GET /xrpc/tangled.ap.resolve?resource=` | `acct:` / actor URL → hosted DID |

Inbox `Create{Note}` writes attested `sys.foreign` (rkey = BLAKE3(`activitypub||0x00||id`)) plus mapped `pub.tangled.post`. `Like` / `Announce` / `Follow` map to `pub.reed.like` / `pub.reed.boost` / `tangled.graph.follow`. Local public put of those collections emits outbound Create/Like/Announce/Follow to known remote inboxes.

`describe.protocols` includes `activitypub` (feature `activitypub`, default on). RSA key: `{data}/ap-rsa.pem`. CI does not need a live Mastodon; `cargo test -p tangled-knot --test ap` signs fixtures.

Disable the facade: `cargo run -p tangled-knot --no-default-features`.

### Local demo (real Mastodon outbox)

```bash
just reed-ap-demo
```

Starts this knot (ActivityPub on, loopback fetch allowed for local advertised URLs), writes Reed keys via `reed-seed --create-local`, follows **`acct:Gargron@mastodon.social`**, GETs that actor’s public **outbox** (and first page), and opens Reed. After Refresh, Home and Discover show real federated posts. No mock Mastodon process.

Override the account (must be a public `acct:user@instance` with a public outbox):

```bash
REED_AP_ACCT=acct:Mastodon@mastodon.social just reed-ap-demo
# or
just reed-ap-seed --acct acct:user@instance
```

`just reed-demo` stays Tangled-only. `just reed-ap-demo-mock` keeps the old loopback peer (inbox POST, no public internet).

Resolve (`GET /xrpc/tangled.ap.resolve`) does WebFinger + actor fetch, then pulls the outbox. HTTP signatures are required only for inbox POST; outbox GET is public. CI never hits the public internet (`cargo test --workspace`). Live pull: `REED_AP_LIVE=1 cargo test -p tangled-knot --test ap live_mastodon_outbox_pull -- --ignored --nocapture`.

### Follow a real Mastodon user (inbound S2S)

Outbound Create/Follow/Like to the instance inbox is still attempted from this knot. **Inbound** replies from Mastodon need a reachable inbox: run the knot with `--public-url https://your-knot.example` (HTTPS in front). In Reed Search paste `acct:user@instance` (or `@user@instance`) and Follow. Without a public URL, you still see posts via the outbox pull above.

## Data directory

```
data/
  identity.json          # knot Ed25519 + X25519 seeds
  describe.json
  config.json            # includes learned_peers (not advertised)
  peers.txt              # optional operator public peer URLs
  identities.json
  heads.json
  repos/<did>/collections/<collection>/<rkey>/<cid>.json
  blobs/<did>/<cid>
  blocks/<cid>
  cap-uses.json
  subscribe.json
```

In-memory maps are the store; the directory is a snapshot after writes. Tests may run with no `data_dir`. SIWS sessions are memory-only (die on restart / migration, spec 17).

## Sessions (SIWS)

```
GET  /xrpc/tangled.session.challenge?did=
POST /xrpc/tangled.session.verify     { did, nonce, signature }
```

Cookie: `tangled_session` (OpenAPI), HttpOnly, Path=/, Max-Age=86400, SameSite=Lax, and `Secure` when `--public-url` is `https://…`. The SIWS message includes domain, DID, nonce, `issuedAt`. Record signatures are still required. Hosted identity-key writes (CLI) remain valid without a cookie.

## Tests

```bash
cargo test --workspace
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Exercise SIWS over HTTP: `cargo test -p tangled-knot siws_http`. Handle fixture: `cargo test -p tangled-knot identity_file_roundtrip`. SNS: `cargo test -p tangled-knot sns_handle_resolve_http`. MLS is in `tangled` (`cargo test -p tangled --lib mls::`).
