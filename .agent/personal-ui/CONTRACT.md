# Personal app contract

User requested usable app interfaces for all eight existing personal workflows, third-party interoperability, subagents in separate worktrees, validation and merge master. Root primary worktree owns integration plus app shell/styles/client helpers; backend and feature panes have disjoint owners.

## Data and service

The existing tangled-personal Store/execute service remains source of truth. No app-private DB. Unknown extensions preserved by existing library workflows. Session-token-protected local browser service is launched by `tangled-personal serve --data DIRECTORY --listen 127.0.0.1:PORT`. Owner bootstrap token is in the startup URL fragment `#session=HEX`, never embedded in public HTML or returned by an unauthenticated endpoint. SessionStorage retains it for tab lifetime; same-origin browser requests use `X-Tangled-Session: HEX`.

HTTP JSON API:
- GET /api/v1/session -> {did:string,features:string[],...}; authenticated, no-store.
- POST /api/v1/personal/:feature with existing execute input -> raw existing execute result JSON; failure -> {error:string}, non-2xx. Features info,records,memory,capture,notes,feeds,agents,devices,publish,connections. This is the same interface for other authorized clients, not UI-only state.
- POST /api/v1/backups/export {passphrase} -> {filename:string,backup:encryptedPacketObject} (browser download, no caller server path).
- POST /api/v1/backups/verify {passphrase,backup:object} -> existing verification status.
- POST /api/v1/backups/sync {passphrase,backup:object} -> existing sync status.
- POST /api/v1/backups/restore {passphrase,backup:object,label:string} -> {restored:status,session:{did,...}}; creates a separate fresh private repository and switches active service Store only after successful restore. Never overwrites active library. Explicit restore location surfaced to owner, restart behavior documented.
- GET /api/v1/manifest -> public versioned discovery/actions/auth/collection schema metadata. No personal info or token. Local UI/assets public, data/session APIs authenticated, strict loopback Host/Origin protection, no ambient credentials/CORS wildcard. Bounded requests; lock owner Store once for server life and serialize command operations. Blocking storage/KDF uses blocking worker. No new live connector sends/remote URL fetch required.

## Browser layout and ownership

Files apps/personal/web/{index.html,app.css,app.js,ui.js} root. Feature owners replace stub library.js,reading.js,manage.js. Backend embeds these seven files as static assets, no runtime dependence on source checkout. App launch must be a normal one-command local browser app, no JSON editing/terminal workflows for features. File pickers/downloads replace JSON/path entry wherever possible.

Feature modules export async mount functions `(root,ctx)`:
- library.js: mountLibrary,mountCapture,mountNotes
- reading.js: mountFeeds,mountConnections
- manage.js: mountAgents,mountDevices,mountPublish

Modules import shared helpers from './ui.js':
`el(tag,attrs={},...children)` returns HTMLElement; attrs supports class,text,value,type,checked,disabled,onClick,onSubmit,onChange,onInput; all text/children escaped via DOM, never untrusted innerHTML. Children strings/nodes/arrays supported.
`field(label,control,hint='')` wraps labeled control and optional hint; auto ID if absent.
`button(label,onClick,kind='secondary')` type=button; kind primary/danger/secondary.
`empty(title,description,actionNode?)` returns empty-state section.
`recordTitle(record)` gives body.title/name/text preview or collection label.
`timeLabel(timestamp)` formats Unix milliseconds envelope timestamps; source strings can be displayed directly.

ctx methods:
- api(feature,input) -> Promise<raw JSON>, throws Error with friendly server text.
- request(path,input) -> Promise<JSON>, POST to /api/v1/{path}, used backups/*.
- notify(text,tone='success') -> global live status; tone error/success/info.
- run(buttonNode,asyncTask) -> Promise<void>, sets disabled/busy during task, catches errors into notify, restores button. Feature panes may also show local inline errors.
- download(filename,data,mime='application/json') -> Blob download; string,object,Uint8Array supported. Objects JSON serialized; bytes retain exact content.
- navigate(view) -> route one of library,capture,notes,feeds,connections,agents,devices,publish.
- refreshSession() -> refreshes ctx.session and shell owner display.
- session current authenticated service metadata.

Feature mount owns supplied root. Use replaceChildren and DOM helpers. Context supplies no test data, fake users or canned success. Reload lists after successful mutations; preserve filled forms on error. No raw JSON textareas as primary feature interface. Attachments use File/Uint8Array, import formats use file picker. Errors name recovery action. Each form labeled, keyboard usable, loading/empty/error/success states. Public source HTML always inert text or sandboxed generated preview.

CSS vocabulary supplied by root: .toolbar,.stack,.split,.pane,.list,.item,.item-main,.item-title,.meta,.actions,.field,.hint,.reader,.reader-body,.table-wrap,.table,.badge,.empty,.inline-status,.section-title,.form-grid,.check,.detail,.danger-text,.muted,.button(.primary/.danger),.selected. Standard semantic elements styled. Avoid inline styles unless needed for specific dynamic behavior; no own CSS files unless coordinate.

## Visual contract

Mode Operate. Extend current app world: white #fff, paper #f7f7f5, black #191919, neutral text #626262, borders #d9d9d4, accent #d32323 sparingly for active/primary/focus. 224px word-based sidebar, 64px top chrome, ruled rows and a useful reader; mobile navigation wraps horizontally. Comfortable 15px system/Fira Sans UI, serif plain-text reader optional. No hero/dashboard metrics, icon-card grid, gradients, emoji icons, raw protocol internals in everyday flow. All controls functional; data stays private by default. Storage custody and irreversible/public copy decisions explained at action.

## Validation / boundaries

Each feature owner: browser module + meaningful tests if self-contained, own doc additions if needed, commit only assigned module/tests. Test with real backend through browser when available; parent owns full Playwright matrix at desktop/mobile + independent code/design review. Backend owner covers HTTP auth/Origin/Host, request limits, persistence, backup wrappers, API interoperability integration tests and CLI server start. Shared Rust target cannot be used concurrently across worktrees; backend may use /tmp/tangled-ui-server-target then parent validates same code sequentially. Frontend agents do not build Rust duplicates.

All main checkout pending user work preserved; never edit /Users/dode/Documents/network/tangled. Root later performs backed-up master merge. Do not spawn extra agents or ask user mid-build. State reasonable defaults in your report. Do not publish/deploy or authorize real external accounts.
