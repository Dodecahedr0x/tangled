---
title: Hosted source pairing
slug: hosted-pairing
description: Configure browser sign-in and supervised Telegram, X, and WhatsApp imports.
order: 7
---

# Browser pairing host

The browser uses Tangled's authenticated import page. The operator runs this
separate native source service explicitly; users do not run connector commands or
copy ingest/source tokens. The service binds **127.0.0.1 only**, requires a control
bearer, rejects browser Origin headers, and does not log request paths or bodies.
Keep this service and Tangled under the same trusted operator. Do not expose its
control port through a public reverse proxy.

Install `connectors/requirements.lock` into a Python virtual environment, and run
`npm ci --prefix connectors/whatsapp` using Node 22 or newer. Use private 0700
state directories. Create independent private random key and control-token files:

```sh
umask 077
mkdir -p /var/lib/tangled-pairing
openssl rand -out /var/lib/tangled-pairing/key 32
openssl rand -hex 32 > /var/lib/tangled-pairing/control-token
```

Run under a supervisor using the installed Python environment. Export these on
the **pairing service** (substitute actual deployment paths and public host URL):

```sh
TANGLED_SECRET_FILE=/var/lib/tangled-pairing/key
TANGLED_PAIRING_TOKEN_FILE=/var/lib/tangled-pairing/control-token
TANGLED_PAIRING_STATE_DIR=/var/lib/tangled-pairing/state
TANGLED_HOST_URL=https://your-tangled-host.example
TANGLED_PAIRING_PORT=8791
```

Then run `python connectors/hosted/server.py`. The server refuses startup without
its private key/token/state configuration. Its encrypted index retains native
sessions and worker bindings across restarts; workers resume the existing source
connectors' durable acknowledged checkpoints. A worker records its PID and random
session identity before it performs source requests; restart/erase stop surviving
workers only after verifying the owned connector path and session identity.
Worker stderr/stdout are suppressed. Workers retry after operational failures;
the browser receives a safe retry status without source response bodies.

Startup, disconnect, and erasure also remove encrypted temporary index fragments
left by interrupted atomic writes. Cleanup preserves canonical records and other
owners' sessions, never follows symlinks, and remains pending if deletion or its
directory durability check fails.

Export these on the **Tangled host**:

```sh
TANGLED_PAIRING_URL=http://127.0.0.1:8791
TANGLED_PAIRING_TOKEN_FILE=/var/lib/tangled-pairing/control-token
```

The control URL must be an HTTP literal loopback address without credentials,
path, query or fragment. Its token must be a private regular non-symlink file
containing at least 32 ASCII characters. The encryption key must contain at
least 32 random bytes. Both files stay off the browser and outside repositories.
An absent service configuration reports unavailable. A configured unreachable
service makes disconnect/erasure pending, because stopped workers and removed
credentials cannot be acknowledged.

Enable the sources the operator intends to offer:

- **Telegram:** `TELEGRAM_API_ID` and a private `TELEGRAM_API_HASH_FILE`, obtained
  for the operator's Telegram application. Browser login uses actual Telethon
  `send_code_request`, `sign_in(phone, code, phone_code_hash)`, and two-step
  `sign_in(password)` if Telegram requests it. The native user ID is authoritative;
  bot accounts are rejected. Phone/hash and StringSession state are encrypted;
  entered codes and passwords are not persisted.
- **X:** `X_CLIENT_ID`, optional private `X_CLIENT_SECRET_FILE`, and
  `X_HOSTED_REDIRECT_URI=https://your-tangled-host.example/api/imports/pairing/callback/x`.
  Register this exact callback in the X application. Production callback URLs must
  use HTTPS; local HTTP callbacks are allowed for loopback development. Browser
  authorization uses source-native OAuth2 S256 PKCE, random expiring owner-bound
  single-use state, server token exchange, and `/2/users/me` identity verification.
  The existing `X_CATEGORIES` configuration chooses requested scopes and
  import categories. The owner must remain signed in on the returning host. Source
  permissions, API access tiers and retention limits still apply.
- **WhatsApp:** Node and the pinned existing Baileys dependencies. Optional
  `TANGLED_NODE` selects the Node executable. Actual `connection.update.qr` events
  become SVG QR images via the existing QR encoder; scan with WhatsApp's linked
  devices UI. `socket.user.id` normalized by Baileys is authoritative. Existing
  encrypted credential/key and pending history spools are reused. Pairing keeps
  capturing supplied history until binding automatically switches to the normal
  ingest worker. Linked-device history is limited to what WhatsApp supplies.

Challenges last ten minutes with at most eight submitted Telegram steps. Owner
isolation applies to status, step, callback, bind, disconnect and scope erasure.
Tangled's private lifecycle mutex serializes pairing with erasure and custody
migration; the host generation captured before native auth is rechecked before
connection issuance. Reconnect revokes old host ingestion capabilities and stops
older workers for that native account. Disconnect removes hosted source sessions
and pending connector state. It does not delete imported archive objects; archive
erasure does that separately. Source-side sessions/device listings may still
need revocation in the source account; the service deletes host-retained
credentials and does not promise remote account deletion or recall backups.

## Container supervision and disabling

The provider image enables hosted pairing by default (`TANGLED_HOSTED_PAIRING=1`). Its supervisor starts the provider and the loopback broker, creates absent private defaults under `$DATA_DIR/pairing-control/`, preserves them across restarts, and shuts down owned children if either service fails. Operator-supplied key/token paths must already exist and pass private-file checks. Telegram/X application credentials remain operator configuration. The image includes the pinned Python/Node adapters; no image build was verified in this development environment because the Docker daemon was unavailable.

`TANGLED_HOSTED_PAIRING=0` disables the broker. When prior source authorization is retained, the provider preserves the original cleanup control configuration and cannot acknowledge erasure or disconnect until the operator re-enables the broker. A missing/corrupt prior key or token fails setup rather than silently replacing secrets. Disabling never deletes other owners' authorization automatically. After all hosted sessions and retained state are cleared, disabling can remove the unused control configuration normally.

## Verification

```sh
PYTHONPATH=connectors python -m unittest discover -s connectors/tests
PYTHONPATH=connectors python -m unittest discover -s connectors/hosted -p 'test_*.py'
node --test connectors/hosted/whatsapp.test.mjs connectors/whatsapp/*.test.mjs
cargo test -p tangled-knot --lib imports::pairing::tests
```

The control tests use a real loopback HTTP server; orphan recovery uses a real
child process. Native source tests isolate remote provider transports and exercise
Telethon challenge interfaces, XClient PKCE/token/user lookup and encrypted
persistence, and the actual installed WhatsApp QR/credential libraries. No live
source authorization has been performed without operator application credentials
and an explicitly authorizing account owner. A passing controlled test does not
claim a source account was paired live.
