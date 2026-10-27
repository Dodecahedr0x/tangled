# Personal

A local browser app with Today and all eight personal workflows, built on the shared signed
repository engine and extensible collections.

From the repository root:

```sh
cargo run -p tangled-personal -- serve --data .data/personal --listen 127.0.0.1:8788
```

Open the private launch link printed by the service. The browser normally opens
automatically; add `--no-open` to choose it yourself. Keep that link private.
Today brings together reminders, inbox notes and unread material. Notes protect
unsaved edits; Copy link opens the same record after reload or from another app.
The server embeds the app assets; no Node build is required to use it.

The [app and API guide](../../docs/features/personal-app-api.md) describes the views,
local session, recovery and third-party integration. Existing app settings link to
`http://127.0.0.1:8788/`; native apps accept `TANGLED_PERSONAL_URL` at runtime, while browser builds use its compile-time value.
An authenticated Personal tab can connect another same-origin tab.

## Development checks

```sh
cd apps/personal
npm ci
npm run check
npm test
# First build tangled-personal at the repository root, then:
npm run test:browser
```

The browser test uses Playwright Chromium and a temporary isolated repository.
Install Chromium with `npx playwright install chromium` if necessary. Set
`TANGLED_PERSONAL_APP` to another binary path and `TANGLED_UI_SCREENSHOTS` to an
output directory for desktop/mobile captures. It covers all eight workflows,
error recovery, downloads, scoped grants, third-party extension preservation and
encrypted restoration. It creates no external accounts and sends no source replies.
