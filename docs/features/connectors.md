---
title: Live connectors
slug: connectors
description: Set up separately operated Telegram, X, and WhatsApp import helpers.
order: 6
---

# Live social import runners

These optional runners authenticate as **your Telegram user account, X user account, or WhatsApp linked device** and send private archive batches to Tangled. They do not use Telegram bots or the WhatsApp Business API. Source credentials remain on the machine running the helper; Tangled receives imported content and the connection-scoped ingest token only.

The archive page offers hosted browser pairing through **Connect app**; see [hosted pairing setup](./hosted-pairing.md). The provider container bundles its private supervised service, which needs operator source application credentials. The external runners described below remain optional: create a connection under **Use an external connector**, then run it on your own computer/private server if source authorization should stay there.

## Install

Python 3.11+ is required for Telegram/X. Node 22+ is required for WhatsApp. From the repository root:

```sh
python3 -m venv connectors/.venv
connectors/.venv/bin/pip install -r connectors/requirements.lock
npm --prefix connectors/whatsapp ci --ignore-scripts
```

The lock files record the versions checked during development. In particular, WhatsApp uses Baileys `7.0.0-rc14`; its protocol and release-candidate API can change. Review upstream updates before upgrading. The source integrations were checked against installed libraries and local protocol tests, **not real source accounts**; account authentication, history coverage, platform restrictions, and media availability still require live validation.

## Local secret and destination setup

Choose a private directory outside the checkout and create a random encryption key. The key is required, including for unattended runs; it is not a password. Store it separately in a secret manager or protected backup. Losing it makes saved sessions, checkpoints, and pending WhatsApp content unreadable.

```sh
umask 077
mkdir -p "$HOME/.config/tangled-connectors"
openssl rand -out "$HOME/.config/tangled-connectors/encryption.key" 32
export TANGLED_SECRET_FILE="$HOME/.config/tangled-connectors/encryption.key"
```

The runners enforce mode `0600` on secret files and `0700` on state directories, encrypt local session/checkpoint/spool files with AES-256-GCM, write them atomically, and permit only one runner per state directory. Keep the key and the encrypted state on persistent private storage, outside repositories. Do not run two instances against the same directory. These protections do not hide data from a process that can read both the key and the state.

Authenticate with the source first using the instructions below. The helper prints the native **source account** identifier. Register that exact value in the Tangled connection: Telegram and X use a numeric user ID; WhatsApp uses a normalized self JID such as `33123456789@s.whatsapp.net`. X uses the source name `twitter` in the wire format. Foreign senders remain source IDs in imported metadata.

Save the ingest token from the Tangled UI without putting it in shell history:

```sh
connectors/.venv/bin/python -c 'import getpass,os,pathlib; p=pathlib.Path.home()/".config/tangled-connectors/ingest-token"; fd=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_TRUNC,0o600); os.write(fd,getpass.getpass("Tangled ingest token: ").encode()); os.close(fd)'
export TANGLED_URL="https://your-tangled-host.example"
export TANGLED_CONNECTION_ID="the-connection-id"
export TANGLED_INGEST_TOKEN_FILE="$HOME/.config/tangled-connectors/ingest-token"
export TANGLED_ACCOUNT="the-exact-native-source-account"
```

Use a separate token file and `TANGLED_STATE_DIR` for each source account/connection. Ingest tokens are read from the file on every batch, so a securely replaced token file can rotate credentials. Production destinations require HTTPS; HTTP is accepted only for loopback development. Redirects are rejected to prevent credentials following a redirect.

The HTTP contract is `POST /api/imports/connections/{id}/ingest`, bearer authorization, and the exact `ImportBatch` fields `source`, `account`, `objects`, `files`, `warnings`. Source/account must match the connection. A 2xx response is treated as durable success. Five bounded retries handle network errors, HTTP 429, and HTTP 5xx; other HTTP failures stop immediately. No response bodies or source exception objects are logged. A revoked destination token therefore stops the runner without advancing its checkpoint. Native message IDs make replay after a crash safe, including a crash after server success but before saving a checkpoint.

The default attachment limit is **8 MiB per file**, configurable with `TANGLED_MAX_MEDIA_BYTES`. Encoded JSON batches have a 128 MiB bound. Files over the attachment limit produce explicit archive warnings and retain their raw source metadata; their bytes are not imported. Download failures stop Telegram/X before advancing the cursor. WhatsApp history media can expire: after five download failures, that message is imported with an explicit unavailable-media warning. Raise the limit only within your destination's configured request limit and available memory.

## Telegram

Create your own Telegram API application at [my.telegram.org](https://my.telegram.org). Save the API hash in a private `0600` file; no bot token is used.

```sh
export TELEGRAM_API_ID="your-numeric-app-id"
export TELEGRAM_API_HASH_FILE="$HOME/.config/tangled-connectors/telegram-api-hash"
export TANGLED_STATE_DIR="$HOME/.local/share/tangled-connectors/telegram-main"
connectors/.venv/bin/python connectors/telegram.py login
```

Enter your phone, login code, and optional two-step verification password in the terminal. The login code and password are hidden. An encrypted Telethon `StringSession` is saved locally; no plaintext SQLite session is created. The helper rejects bot sessions and prints your numeric Telegram user ID.

After configuring the matching destination:

```sh
connectors/.venv/bin/python connectors/telegram.py run
```

`once` imports available history and exits after a complete pass. `run` imports available cloud history oldest-first in each dialog, then polls all dialogs every 60 seconds by default (`TANGLED_POLL_SECONDS`, minimum 15). It includes both your messages and foreign messages visible in those dialogs, original metadata, original timestamps, and downloadable photos/documents/audio/video. New dialogs are discovered each pass. Each cycle also archives the current own-profile and contact/user metadata snapshots; a source failure fetching contacts is recorded as a warning while message history continues. The native object ID is `dialog_id:message_id`, because Telegram message IDs are scoped to dialogs. A per-dialog forward cursor is saved only after destination acknowledgment. The most recent 100 message-ID positions are revisited to catch recent edits.

Limits: Telegram secret chats, deleted messages, content the account cannot access, and edits outside that overlap cannot be recovered. Large histories can take a long time and cause flood waits. Source operations have bounded retries; waits over five minutes stop the process for a later restart. Polling sends no messages, read receipts, contact changes, or join requests. Authentication and Telegram session maintenance still use the source protocol.

References: [Telethon sessions](https://docs.telethon.dev/en/stable/concepts/sessions.html), [client history/media API](https://docs.telethon.dev/en/stable/modules/client.html).

## X

Enable user OAuth2 for an X developer application with the exact callback `http://127.0.0.1:8765/callback` (or set another loopback port in `X_REDIRECT_URI`). API access/billing must permit `/2/users/me` and whichever read categories you enable. By default, the runner attempts all implemented categories: `posts,dms,likes,bookmarks,followers,following,profile`. Set `X_CATEGORIES` to a comma-separated subset before login and during operation to limit imports and requested permissions.

```sh
export X_CLIENT_ID="your-app-client-id"
export TANGLED_STATE_DIR="$HOME/.local/share/tangled-connectors/twitter-main"
connectors/.venv/bin/python connectors/x.py login
```

Open the printed authorization URL on the same computer as the runner. The loopback callback validates a random state and uses PKCE S256. Only read and offline scopes are requested: the default is `tweet.read users.read offline.access dm.read like.read bookmark.read follows.read`. An `X_CATEGORIES` subset requests only its required category scopes plus the basic user/post/offline scopes. Existing tokens missing a category scope produce an explicit archive warning for that category; reauthorize to add permissions. Login expires after five minutes. Confidential apps also require `X_CLIENT_SECRET_FILE` pointing to a private `0600` file. Access and rotating refresh tokens are encrypted locally.

For a remote unattended worker, perform login on a trusted local computer and securely transfer the encrypted state and its key, or use an SSH loopback tunnel for the callback. Keep callback URLs exact. Do not paste callback codes into logs.

Alternatively, set `X_ACCESS_TOKEN_FILE` to an existing **OAuth2 user** access token in a private file before `login`. An app-only bearer token is insufficient and will fail `/users/me`. Optionally supply `X_REFRESH_TOKEN_FILE` and `X_CLIENT_ID` to establish refreshable credentials; tokens without a refresh token require reauthorization when they expire.

```sh
connectors/.venv/bin/python connectors/x.py run
```

`once` imports available pages of every selected category and exits; `run` then polls every 120 seconds by default (`TANGLED_POLL_SECONDS`, minimum 60). This includes the authenticated user's posts/replies/API-returned reposts; available legacy DM messages and conversation events; liked/bookmarked posts with available content/media; follower/following user metadata; and profile snapshots. Native IDs align with archive imports: post/event IDs remain native; relation IDs are `like:<postId>`, `bookmark:<postId>`, `follower:<userId>`, and `following:<userId>`. The source does not provide when a like/bookmark/follow occurred, so relation timestamps stay unknown rather than borrowing the target post's creation time.

Each category keeps separate pagination state. Own posts preserve the old `since_id` until every older page succeeds. The other categories rescan their currently available snapshots each cycle, because a user can like an old post or follow an old account; target IDs cannot safely serve as new-event cursors. A page checkpoint is saved only after destination acknowledgment. Permission, entitlement, and source failures produce explicit category warnings and allow the other categories to continue. Destination rejection stops the entire runner without advancing the failed page. Partial API errors and raw error metadata are retained; affected category cursors stay unchanged for retry.

Available photos and the highest-bitrate returned MP4 variant are downloaded from X's media CDN. Authenticated DM media is downloaded when the response supplies the documented `/2/dm_conversations/media/...` API URL. Unsupported legacy media URLs or missing download coordinates produce explicit warnings; no private token is sent to an unapproved media host. Long-form `note_post`/legacy `note_tweet` text is used when returned. Original objects, unknown fields, expansions, source times, foreign senders, and source errors remain archived.

Limits: the currently documented user timeline returns at most the latest 3,200 posts, and legacy DM lookup exposes at most 30 days of events. New encrypted X Chat is a separate protocol and is not imported by this runner. Likes, bookmarks, followers, and following are available-current-state snapshots, not a recovered history of removed relationships. Snapshot changes can race pagination; absence does not delete archived evidence. Protected/deleted content, media, edits, and category access depend on permissions and X entitlements. Full snapshot rescans can use substantial API capacity; increase `TANGLED_POLL_SECONDS` or reduce `X_CATEGORIES` for large accounts. A stale pagination token may require resetting only the encrypted cursor file (after stopping the runner) to replay backfill; do not delete `auth.enc` unless reauthorizing.

References: [X OAuth2/PKCE and scopes](https://docs.x.com/fundamentals/authentication/oauth-2-0/authorization-code), [X user timelines](https://docs.x.com/x-api/posts/timelines/introduction), [DM events and retention](https://docs.x.com/x-api/direct-messages/lookup/introduction), [likes](https://docs.x.com/x-api/users/get-liked-posts), [bookmarks](https://docs.x.com/x-api/users/get-bookmarks), [followers/following](https://docs.x.com/x-api/users/follows/introduction), [authenticated DM media](https://docs.x.com/x-api/direct-messages/download-dm-media).

## WhatsApp

```sh
export TANGLED_STATE_DIR="$HOME/.local/share/tangled-connectors/whatsapp-main"
node connectors/whatsapp/index.mjs login
```

Run in an interactive private terminal. Scan the QR using WhatsApp → Linked devices → Link a device. The QR is a login credential: do not send screenshots of it or capture terminal output in a shared log. No Business API account is needed. The helper prints your normalized self JID after the protocol opens successfully. Its `login` mode stays running to save initial history locally in an encrypted spool; allow synchronization to run, then press Ctrl-C. Initial sync may be partial and does not have a reliable lifetime-history completion signal.

Configure a destination with that account, then run:

```sh
node connectors/whatsapp/index.mjs run
```

The runner replays encrypted pending history and imports new messages, media, history metadata, chat/contact metadata, edits/deletions as source events, and reactions. The native message ID is `remoteJid:messageId`. WhatsApp has no user-supplied pagination cursor here: received raw events are durably encrypted before download/delivery, and removed from the pending queue only after destination acknowledgment. Dependency console output is disabled because libsignal can log complete private Signal sessions independently of Baileys' configured logger. The runner emits only deliberate status and QR output. Signal/session keys and credentials are saved atomically in an encrypted auth store, including `app-state-sync-key` conversion required by Baileys. A source account or destination connection change fails closed for existing state.

The helper sends no user messages, reactions, read receipts, contact changes, or presence updates. It performs the linked-device authentication, history sync, acknowledgments, encryption-key updates, and reconnect operations required by WhatsApp. It does not request expired media reuploads from peers. `run` refuses new QR authorization; use the interactive `login` mode if the device is unlinked.

Limits: WhatsApp decides which linked-device history is supplied. Complete lifetime history, expired media, view-once content, disappearing messages, offline gaps, and edits/deletions missed while unlinked are not guaranteed. Messages already acknowledged by WhatsApp but not yet emitted by the library can be lost on a process crash; the local spool protects events after delivery to this helper. The pending queue is bounded to 10,000 events or 128 MiB: keep the destination available and monitor the worker, because source events arriving after the cap cannot be archived. A logged-out device needs pairing again: stop the worker, remove only `auth.enc` from this account's state directory, and run `login` again. Keep `source.enc`, `pending.enc`, and the encryption key to preserve the account/destination binding and queued imports. Protocol compatibility and account restrictions remain external constraints.

After an unclean shutdown, remove `runner.lock` **only after verifying the recorded process is gone**; then restart with the same secret/state. After a destination migration, do not silently repoint this state: drain the old destination first and migrate the private archive, or provision a new linked-device state/connection. Existing encrypted pending data remains bound to its original destination.

References: [Baileys project](https://github.com/WhiskeySockets/Baileys), [pinned release](https://github.com/WhiskeySockets/Baileys/releases/tag/v7.0.0-rc14), [upstream auth-state handling](https://github.com/WhiskeySockets/Baileys/blob/v7.0.0-rc14/src/Utils/use-multi-file-auth-state.ts).

## Unattended operation and verification

Perform interactive authorization once, then run the `run` command under your service manager as a dedicated unprivileged user with a persistent private state directory. Supply configuration using a private environment file containing paths, source app ID/client ID, account ID, and destination URL/connection ID. Mount key/token/API-secret files separately. A systemd service can use `Restart=on-failure`, `RestartSec=300`, `UMask=0077`, `NoNewPrivileges=yes`, and a writable state directory. Do not log the interactive login terminal. Disconnecting in Tangled invalidates the ingest token; revoke the source session separately through Telegram/X/WhatsApp account settings if you want source authorization removed too.

Local tests use a real loopback HTTP endpoint for Python authorization/retry/checkpoint behavior and actual installed Baileys protobuf/auth types for encrypted restart checks. They do not contact source accounts:

```sh
connectors/.venv/bin/python -m unittest discover -s connectors/tests -v
connectors/.venv/bin/python -m compileall -q connectors/common.py connectors/telegram.py connectors/x.py connectors/x_categories.py
npm --prefix connectors/whatsapp test
npm --prefix connectors/whatsapp run check
```

Before production, use a dedicated connection for each account and verify login, a small historical conversation/post set, attachment bytes, foreign senders, a new incoming/outgoing message or post, source downtime, a runner restart, destination downtime/replay, and token revocation. These are required live-account checks still outstanding from development.
