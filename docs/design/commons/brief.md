# Tangled suite redesign — Commons

Design proposal · 26 October 2026

## Intent

Give Reed, Loomed, Weft, and the hosted provider a coherent, approachable interface. A person should recognize the family, understand which identity they are using, and complete the task without learning protocol terminology. The user selected a whole-suite redesign and then selected Commons from the visual directions.

This deliverable is a visual prototype and an implementation brief. It does not change the application source, connect to a knot, sign records, generate identities, or submit credentials. All people, posts, conversations, network graphs, and counts in the prototype are synthetic and are labeled as such.

## Approaches considered

- **Extend the incumbent timetable:** retain the white sheet, black rules, and red markers. The user chose a full redesign instead.
- **Index:** yellow navigation, compact content, and directory-inspired discovery. Distinctive and functional, but visually assertive for long reading sessions.
- **Commons — selected:** green navigation, pale green selection, cool light reading surfaces, familiar controls, and a persistent identity and host anchor. This favors comprehension across social reading, private messaging, technical inspection, and account administration.
- **Familiar product UI:** neutral navigation and blue actions. A conventional alternative with less distinctive family identity.

## Product facts and boundaries

- Reed is a public social app: feed, discovery, profiles, replies, boosts, likes, composition, search, settings, and local identity setup.
- Loomed is a private messenger with sealed circles, inbox search, conversation threads, new conversations, local identities, and host settings. The UI does not expose cryptographic scheme selection.
- Weft is a read-only explorer of a configured knot and its advertised public peers. Network totals describe that observed view, never the whole network. Private repositories, sealed contents, and learned peers are excluded.
- The provider creates an account with email, name, and password, generates the identity, holds its keys, and can sign as that account. It also supports public posting and host migration.
- Reed and Loomed currently generate or import local identities. A provider cookie is not an app login. The suite switcher changes destination, not account ownership, and may not promise single sign-on or identity transfer.
- No invented wallet login, global search, cross-app credentials, production users, live network metrics, attachments, media uploads, saved posts, password recovery, account deletion, or email verification.
- Keep the Rust/Dioxus app architecture and the provider's server-rendered HTML form architecture. The visual redesign requires no new frontend framework or new protocol feature.
- The public documentation site and its established URLs are outside this design scope.

## Shared visual system

| Role | Specification |
|---|---|
| Navigation | Deep evergreen `#153e38`; foreground `#f3f8f4` |
| Active navigation | Pale leaf `#d8edc8`; evergreen text |
| Canvas | Cool light `#f3f6f5` |
| Reading/form surface | White `#ffffff` |
| Main text | Dark green-black `#203b35` |
| Secondary text | `#52665c`, verified against its actual background |
| Dividers | `#dce5df`; stronger input borders `#a4b6aa` |
| Main action | Evergreen with white text |
| Warning | Pale amber ground, dark brown text, explicit recovery action |
| Error | Dark red text plus a label; color never carries meaning alone |
| Type | Native UI sans; 16px reading text, 14px controls, 12–13px metadata, 28–32px page titles |
| Layout | 224–232px navigation rail; fluid work area; optional 280–320px contextual area |
| Geometry | 8px buttons, 7px fields, 10px bounded reading/form surfaces; no nested ornamental cards |
| Spacing | 4px base; 8/12/16/24/32px groups; 44px touch targets |

The recognizable move is an identity-and-host anchor. It names the identity, the ownership of the keys, and the host in a stable place. A quiet line connects replies within a public thread; familiar message grouping carries the same continuity into Loomed. These signals carry information rather than decorate the page.

## Screen design

### Reed

- **Home:** Following/Discover navigation, public composer, readable posts, Reply/Boost/Like controls, and contextual identity information. Names dominate metadata. Titles and handles wrap safely; identifiers have copyable full forms in details.
- **Discover:** local discovery and people rows with clear Follow actions. It does not imply a global recommendation service.
- **Profile:** name, identity, follow relationship, profile text, and authored public posts. No fabricated reputation or follower totals.
- **Thread:** original post, reply hierarchy, and an inline public reply composer. A subtle connector preserves conversation structure.
- **Compose:** persistent Public label, text area, existing character limit from source, explicit Post action, and visible pending/error feedback. Draft survives a failed attempt.
- **Search:** query field, local scope explanation, results, and a query-specific empty state.
- **Settings:** editable host URL and connection feedback; explain exactly which host is used.
- **Identity:** separate Generate and Import paths. Warn before replacing local identity; explain local key custody and backup limitations without promising recovery that does not exist.

### Loomed

- **Inbox:** searchable conversation list with participants, last message, and an explicit unread label. On desktop the selected conversation can occupy the adjacent pane; mobile opens a dedicated thread.
- **Conversation:** participant name and member count, grouped messages, timestamps, existing receipt data, and persistent composer. Private scope is visible. Never invent delivery/read guarantees from missing receipt data.
- **New conversation:** recipient identity inputs and a first message. Advanced circle identifiers remain in details rather than dominating the first view. Validate against existing core behavior.
- **Settings:** host configuration and connection state, matching Reed's structure.
- **Identity:** local generation/import with the same wording, replacement caution, and ownership treatment as Reed.

### Weft

- **Overview:** configured host, observation scope, last observed state when available, and the actual node statistics: knots, identities, records, blobs, circles, foreign records, and commits. Unknown values show as unavailable rather than zero. No invented time series.
- **Topology:** map of the configured knot, its advertised public peers, and hosted public identities. Distinguish node types by shape and text as well as color. Pair the graph with a keyboard-readable list and selected-node details. No inferred private edges.
- **Lookup:** DID, handle, or knot URL input, type-specific result details, and clear no-match/error states. Full identities remain selectable and copyable.
- **Settings:** configured knot URL, explicit refresh, and observed connection feedback. No account setup required.

### Provider

- **Create account:** name, email, password; clear sign-in alternative. Before submission, state that this host generates and keeps the keys and can sign as the account. Minimum password length remains the existing eight characters.
- **Sign in:** existing email/password form with field labels and server-returned error presentation. No password-reset link without a recovery mechanism.
- **Account home:** account name and full identity, host custody disclosure, public composer, public post list, Change host, and Sign out. Links to the apps explain their separate local identity setup.
- **Change host:** explain that the identity stays the same and the old host stops signing when the move completes. Issue the existing 15-minute code only through the existing explicit action.
- **Move code:** selectable code and source host, clear 15-minute validity statement, handling guidance, and a return path. No invented countdown unless the backend supplies the actual expiry.
- **Move here:** source host URL, move code, new password, and explicit Move here submission. Use the existing handler and its existing session/result semantics.
- **Moved account:** the old host's stopped-signing state, the retained identity, and a clear destination when returned by the backend. Do not imply a reversible migration or display another active posting form on the old host.

## Responsive behavior

- **Wide desktop:** green rail, main task, optional context. Reed uses a bounded reading measure; Loomed divides list and conversation; Weft gives its map the main area.
- **Tablet:** hide the secondary context at widths up to 1200px, preserve primary navigation, and keep main content fluid. Essential settings and identity details remain available through their dedicated routes. No fixed-width desktop canvas in the finished design.
- **Mobile, 320–430px:** a compact app header and accessible navigation replace the tall rail. The suite switcher stays reachable, and a compact identity/host line preserves the ownership information. Each route has one primary task; conversations open from the inbox with a Back action. No mobile requirement for side-by-side panes.
- Forms use a single column. Buttons wrap or fill available width intentionally. Long DIDs, host URLs, errors, and participant names wrap inside their containers.

## Shared state and interaction contract

- Loading: stable layout, plain status, no fabricated zeros or success badges; skeleton motion is optional and disabled for reduced motion.
- Empty: explain what is empty and show the appropriate next action. Distinguish no local content from no connection and no query match.
- Offline/error: preserve drafts and cached content; show the failed host and an explicit Retry or Settings path. A retry may never fabricate a connection.
- Submission: visible pending state, prevent duplicate submission, preserve entered content after recoverable error, focus a concise error summary. Provider forms remain ordinary server forms.
- Selection and focus: visible focus ring, logical document order, labels on every field, native keyboard behavior, and `aria-current` on current navigation.
- Motion: restrained 140–180ms color/position feedback; no entrance animation that hides content, no persistent pulsing, and respect reduced-motion preferences.
- Status: announce discrete results with a polite live region and errors with an alert; unread and selected states have text or shape cues in addition to color.

## Implementation handoff

Use the existing app route/view split, preserve core calls and signing boundaries, and apply the shared tokens consistently to the existing three app stylesheets plus provider page layout. Shared visual components may be extracted only where the existing architecture benefits: navigation, host status, identity anchor, field, button, empty state, and post/message metadata.

The prototype deliberately simulates navigation and local form feedback. Production persistence, account replacement, cryptographic operations, posting, migration, and network requests must continue through their current code paths. Any proposed interface change requiring a new backend contract is out of scope until explicitly designed.

Builder discretion: exact responsive breakpoint values, Dioxus component extraction, icon library choice if icons are added, and accessible graph implementation details. Keep the selected visual hierarchy and ownership distinctions.

## Review and acceptance

The design is reviewable when every listed route is represented, the four primary surfaces read as one family, desktop and mobile layouts remain legible, synthetic data is labeled, and the account/key distinctions remain truthful. Validate prototype navigation and recovery states, inspect desktop and narrow captures, and review the brief for material implementation ambiguities.

Production implementation is a separate next step after review of this design package.
