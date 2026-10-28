# tangled-provider

A hosted knot. A person creates an account with an email, a name, and a password. The process generates their `did:sol`, stores the seeds on disk, and writes public posts for them. `GET /xrpc/tangled.node.describe` sets `custodialSign: true`: this host can write as the account.

Self-hosting a knot is unchanged. This binary is the path where the person does not run one.

## Run

```bash
just provider
# http://127.0.0.1:8787
```

`DATA_DIR` (default `./data-provider`) holds `operator.json`, `accounts.db`, and the knot snapshot. `PORT` defaults to `8787`; `BIND_ADDR` defaults to `0.0.0.0`. The managed local demo sets `BIND_ADDR=127.0.0.1` so requests reach its own loopback listener. `PUBLIC_URL` or `RENDER_EXTERNAL_URL` is the URL written into identity endpoints. Cookies are `Secure` when that URL is `https`.

## Render

`render.yaml` at the repo root is a paid web service (`starter`) with a 1 GB disk mounted at `/var/tangled`. Render sets `RENDER_EXTERNAL_URL`. Health check is `/healthz`.

The disk is single-instance. A deploy interrupts the process for a moment. Do not restore a disk snapshot onto a running sqlite file; stop the service first.

Point the static site at the provider with `PUBLIC_PROVIDER_URL` when building `site/` (the landing’s “Create an account” button).

Reed, Loomed, and Weft are static sites in the same Render blueprint. Their build sets `TANGLED_KNOT` to this service’s public URL, so a fresh browser session talks to the hosted knot. Desktop builds leave that variable unset and still default to `http://127.0.0.1:3000`.

## Social activity imports

While signed in, **Your archive & connections** opens the private archive at `/imports`. Upload Telegram/X/WhatsApp exports, browse/search content, download original files and supplied media, or create a source-scoped connection for a separately operated live helper. The provider holds account keys and can read imported data. See [social imports](../../docs/features/social-imports.md) for supported formats, privacy, independent recovery backups, limits, and assumptions; see [connector setup](../../connectors/README.md) for real source login and unattended operation.

## Change host

While signed in, **Change host** issues a 15-minute code. On the new host, **Move** takes that code, the old host’s URL, and a new password. The location move preserves the account keys and DID; it transfers custody and the private archive through a destination-bound encrypted handoff. The source fences signing and connector ingestion during preparation, then wipes its custody seeds only after the destination persists and verifies the archive and acknowledges receipt. Interrupted acknowledgements can be resumed with the same move details and destination password. Source-service sessions are excluded; reconnect live helpers on the destination.
