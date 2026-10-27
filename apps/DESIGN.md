---
name: Tangled Commons applications
description: "Runtime visual system for Reed, Loomed, Weft, and the hosted provider."
colors:
  rail: "#153e38"
  leaf: "#d8edc8"
  canvas: "#f3f6f5"
  surface: "#fff"
  text: "#203b35"
  muted: "#52665c"
  line: "#dce5df"
  input: "#a4b6aa"
  focus: "#427c6e"
  rail-text: "#f3f8f4"
  rail-description: "#bdd4c7"
  rail-link: "#e2eee8"
  rail-hover: "#2a5048"
  anchor-line: "#4b6d61"
  anchor-text: "#c0d6ca"
  scope-bg: "#e8eeea"
  scope-ink: "#3e5e49"
  private-bg: "#eeeaf6"
  private-ink: "#574675"
  warning-bg: "#fbf4df"
  warning-ink: "#70511d"
  warning-line: "#e0d7b9"
  danger: "#9b302b"
  error-bg: "#fbefed"
  error-line: "#e2c6c0"
  placeholder: "#60756a"
  rail-input: "#234d43"
  rail-input-line: "#719584"
  rail-placeholder: "#d0e1d5"
  primary-hover: "#25594e"
  secondary-hover: "#eef4ef"
  selection: "#edf4e9"
  fixture-bg: "#e6eee9"
  fixture-ink: "#344d41"
  feed-current: "#52785c"
  federation-ink: "#3f654c"
  quiet-surface: "#edf3ee"
  reply-line: "#cbd9cb"
  message-incoming: "#eef2ef"
  message-own: "#dfeecf"
  graph-edge: "#a9bdb0"
  graph-node-line: "#496e54"
  graph-hosted: "#e5eeea"
  provider-ghost-line: "#6f9184"
  provider-anchor-line: "#51736a"
typography:
  metadata-small:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.55
  metadata:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.55
  secondary:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.55
  control:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "14px"
    fontWeight: 550
    lineHeight: 1.4
  title-small:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.55
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.55
  context-title:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "17px"
    fontWeight: 700
    lineHeight: 1.25
  stat-mobile:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "18px"
    fontWeight: 550
    lineHeight: 1.55
  section:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "19px"
    fontWeight: 650
    lineHeight: 1.35
  stat:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "20px"
    fontWeight: 550
    lineHeight: 1.55
  empty-title:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "21px"
    fontWeight: 600
    lineHeight: 1.55
  thread-profile:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "23px"
    fontWeight: 600
    lineHeight: 1.25
  avatar-large:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "24px"
    fontWeight: 600
    lineHeight: 1.55
  page-mobile:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "26px"
    fontWeight: 650
    lineHeight: 1.25
  brand-mobile:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "27px"
    fontWeight: 650
    lineHeight: 1.55
  page:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "29px"
    fontWeight: 650
    lineHeight: 1.25
  provider-brand:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "30px"
    fontWeight: 700
    lineHeight: 1.2
  brand:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "32px"
    fontWeight: 650
    lineHeight: 1.55
  provider-page:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "34px"
    fontWeight: 650
    lineHeight: 1.2
  identifier:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "12px"
    lineHeight: 1.55
rounded:
  badge: "5px"
  field: "7px"
  control: "8px"
  surface: "10px"
  message-tail: "2px"
  avatar: "50%"
spacing:
  space-1: "4px"
  space-2: "8px"
  space-3: "12px"
  space-4: "16px"
  space-5: "20px"
  space-6: "28px"
  space-7: "40px"
components:
  button-primary:
    backgroundColor: "{colors.rail}"
    textColor: "{colors.surface}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "9px 15px"
    height: "44px"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.rail}"
    rounded: "{rounded.control}"
    padding: "9px 15px"
    height: "44px"
  field:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.field}"
    padding: "10px 12px"
  panel:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.surface}"
    padding: "24px"
  scope-private:
    backgroundColor: "{colors.private-bg}"
    textColor: "{colors.private-ink}"
    rounded: "{rounded.badge}"
    padding: "4px 9px"
  navigation-current:
    backgroundColor: "{colors.leaf}"
    textColor: "{colors.rail}"
    rounded: "{rounded.control}"
    padding: "11px 14px"
---

# Design System: Tangled Commons applications

## Overview

**Creative North Star: Commons.** Evergreen navigation, leaf selection, white reading surfaces, native UI sans, and a stable identity/host anchor make the application family approachable. Names and tasks lead; protocol identifiers remain available and wrap safely.

This is the runtime application authority: shared CSS is `apps/commons.css`, content layouts are in each app’s `assets/app.css`, and provider styles are in `crates/tangled-provider/src/commons.css`. The root DESIGN.md continues to govern the documentation website. Prototype snapshots under `docs/design/commons` are design history, not the current runtime contract. This record is derived from implementation code; it does not claim browser or visual inspection.

**Key Characteristics:** readable density; flat divided surfaces; native controls; explicit key custody and observation scope. The frontmatter records observed values; `.impeccable/design.json` adds motion and breakpoint metadata.

## Colors

**Primary:** `rail` grounds navigation and main actions; `leaf` selects the current destination; `selection` quietly marks suite/current content. Rail foreground and description tokens are tuned for the dark ground.

**Neutral:** `canvas` surrounds white `surface`; `text` and `muted` establish reading hierarchy. `line` divides content; `input` defines controls. Rail fields use their own dark surface, stronger border, and light placeholder.

**Semantic:** warnings and errors combine explicit copy with their tinted grounds and borders. Lavender identifies private scope. Incoming and own message colors pair with alignment and a trailing corner. Federation, reply, and graph tokens have specific content roles; they are not extra action accents. Fixture colors belong only to explicitly labeled fixtures.

**The Scope Rule.** Selection never substitutes for public/private labels, key custody, host address, or the explorer’s public observation scope.

## Typography

Use the native UI sans stack already selected by the user. Technical identifiers use system monospace. The type steps in frontmatter include content-specific roles rather than forcing every app into one heading scale.

Shared app titles use (29px, 650, 1.25); mobile titles use (26px). Brand text changes from (32px) to (27px). Body copy is (16px/1.55); Reed post paragraphs use line height (1.6). Shared field labels are (14px, 550), metadata (11–13px), and mobile input text is (16px). Reed profile names and Loomed thread titles use (23px); Loomed mobile thread titles use (21px). Weft statistics use tabular numbers (20px; 18px mobile).

The provider deliberately uses larger task titles (34px/1.2; 29px mobile), a (30px) brand becoming (24px), (19px) section headings, and (13px) identifiers. Preserve these actual app differences. Reed profile text caps at (65ch); Loomed messages cap at the smaller of (65ch) and (82%) on desktop.

## Layout

Shared apps use a flex shell: evergreen rail (232px), flexible stage, white suite bar (minimum 72px). The stage is a column with padding (30px 34px 48px) and gap (18px). Bounded task panels cap at (760px); a padded stage wrapper can remain full width. Keep `.stage`, `.stage-panel`, and `.stage-pad` roles distinct: flex shrinking, padding, and panel bounds belong to the wrapper used by the actual route.

- At (1000px and below), the shared rail becomes (195px) and stage padding becomes (26px 24px).
- At (650px and below), the shell stacks, rail navigation wraps, and the identity anchor stays visible as a compact wrapping strip. Stage padding becomes (22px 16px 32px), gap (16px), and panel padding (19px). The suite bar reaches minimum height (58px).
- Reed Home has a main/context grid with a (285px) context column and gap (26px). `.home-context` is hidden at (1200px and below), not moved below the feed. Keep essential identity and host facts in the persistent rail anchor and their own routes. Mobile feed text/actions reclaim the avatar column; quoted text cancels that offset.
- Loomed uses separate inbox/thread routes, a sticky thread composer, and message grouping. Desktop messages cap at (82%); mobile messages widen to (94%), with smaller log padding. Do not import the prototype’s side-by-side inbox layout.
- Weft topology uses a (265px) detail column and gap (24px), becoming one column at (1100px and below). Its graph canvas is (440px) high, reduced to (330px) on mobile. Public node details remain available as ordinary document controls.
- Provider uses a (232px) grid rail, (620px) task measure, (76px) suite bar, and main padding (40px 34px 48px). At (900px and below) its rail becomes (180px); at (650px and below) the shell stacks, main padding is (26px 16px 36px), and suite height is at least (60px).

Use the shared spacing tokens for rhythm and retain documented component-specific padding. Let action rows wrap, long identities/hosts break anywhere, fluid content shrink, and text areas resize vertically.

## Elevation & Depth

The runtime family has no decorative drop shadows. White surfaces, tinted state areas, and single-pixel borders carry depth. Buttons and app navigation transition background/text color for (150ms ease-out), only under `prefers-reduced-motion: no-preference`. Do not add pulsing loading or entrance motion.

## Shapes

Reusable shapes are recorded in frontmatter: badges, tighter fields, softly curved controls, and bounded surfaces. Avatars are circular; Reed provides (28/40/48/70px) avatar sizes. Loomed bubbles use surface corners with one (2px) trailing corner. Thin reply branches and table/list dividers organize relationships without nested ornamental cards.

## Components

- **Buttons:** evergreen primary, white bordered secondary. Shared buttons are at least (44px) high with (9px 15px) padding; Reed post actions are compact (40px). Provider buttons use (10px 16px), weight (600). Shared disabled opacity is (.55). Hover uses the exported main/secondary colors.
- **Navigation:** current routes use leaf plus `aria-current`; suite switching remains distinct from account identity. Shared rail targets are (46px), mobile (44px); provider rail targets are (48px), mobile (44px). Dark rail fields and their placeholders use rail-specific contrast tokens.
- **Fields:** shared padding (10px 12px), minimum height (44px), text areas (140px). Provider fields use (11px 12px), minimum height (46px), text areas (150px). Preserve visible labels, help, native keyboard behavior, and text errors.
- **Panels/lists:** white surfaces and single-pixel borders; shared task panels pad (24px), Reed context/composers (22px), and provider panels (22px). Mobile panels use (19px). Feed/conversation rows share bounded containers.
- **Messages/metrics:** keep names, timestamps, unread/private text, and real receipt information visible. Weft numbers describe the configured host’s public view, not global network totals; graph labels/details carry information alongside color.
- **Identity/host anchor:** Reed and Loomed describe keys on this device, provider describes keys held by the host, and Weft describes read-only public observation. Suite links do not promise shared login or identity transfer.

**The Visible Focus Rule.** Keep the (3px) focus outline and (3px) offset, skip links, labeled fields, current/pressed states, and readable disclosures. The shared rule covers summaries as well as ordinary controls. Retain any error/status announcements supplied by the app; this documentation does not certify WCAG conformance.

## Do's and Don'ts

- **Do** preserve native type, readable spacing, visible focus, wrapping identifiers, and custody anchors at narrow widths.
- **Do** distinguish real observations, unavailable data, and explicitly labeled fixtures.
- **Do** preserve current route/component architecture and application-specific measures.
- **Don't** apply the documentation site’s copper/slate/Fira system to these applications.
- **Don't** infer global network scope, shared sign-in, key transfer, or delivery guarantees from visual grouping.
- **Don't** use prototype-only copy or claim this code-derived record was visually inspected.
