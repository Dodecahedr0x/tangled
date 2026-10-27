---
name: Tangled Commons
description: "Visual system for the 24-screen Tangled suite design prototype."
colors:
  rail: "#153e38"
  leaf: "#d8edc8"
  canvas: "#f3f6f5"
  surface: "#fff"
  text: "#203b35"
  muted: "#52665c"
  line: "#dce5df"
  input: "#a4b6aa"
  amber: "#fbf4df"
  amber-ink: "#70511d"
  red: "#9b302b"
  focus: "#427c6e"
  rail-text: "#f3f8f4"
  primary-hover: "#25594e"
  button-hover: "#eef4ef"
  suite-selected: "#edf4e9"
  private-bg: "#eeeaf6"
  private-ink: "#574675"
  error-bg: "#fbefed"
  error-border: "#e2c6c0"
  message-incoming: "#eef2ef"
  message-own: "#dfeecf"
typography:
  page:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "29px"
    fontWeight: 650
    lineHeight: 1.2
    letterSpacing: "-0.7px"
  form-title:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "34px"
    fontWeight: 650
    lineHeight: 1.2
    letterSpacing: "-0.7px"
  section:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "19px"
    fontWeight: 650
    lineHeight: 1.35
    letterSpacing: "-0.2px"
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.55
  post:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.6
  control:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "14px"
    fontWeight: 550
    lineHeight: 1.4
  label:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "13px"
    fontWeight: 600
    lineHeight: 1.55
  metadata:
    fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.55
  identifier:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.8
rounded:
  badge: "5px"
  field: "7px"
  control: "8px"
  surface: "10px"
  circle: "50%"
spacing:
  s4: "4px"
  s8: "8px"
  s12: "12px"
  s16: "16px"
  s18: "18px"
  s20: "20px"
  s22: "22px"
  s24: "24px"
  s28: "28px"
  s34: "34px"
  s48: "48px"
components:
  button-primary:
    backgroundColor: "{colors.rail}"
    textColor: "{colors.surface}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "9px 15px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.rail}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "9px 15px"
  button-secondary-hover:
    backgroundColor: "{colors.button-hover}"
  field:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.field}"
    padding: "11px 12px"
  panel:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.surface}"
    padding: "22px"
  navigation-selected:
    backgroundColor: "{colors.leaf}"
    textColor: "{colors.rail}"
    rounded: "{rounded.control}"
    padding: "12px 15px"
  badge-private:
    backgroundColor: "{colors.private-bg}"
    textColor: "{colors.private-ink}"
    rounded: "{rounded.badge}"
    padding: "4px 9px"
---

# Design System: Tangled Commons

## Overview

**Creative North Star: Commons.** An approachable civic reading room: evergreen navigation, pale leaf selection, white reading areas, and native system type. Names and tasks lead; technical identity information remains available without taking over the page.

This captures the finished, self-contained **24-screen design prototype** across Reed, Loomed, Weft, and the hosted account. Navigation and form feedback run locally with labeled synthetic data. Production application sources and backend behavior are outside this artifact.

Key characteristics: readable density; flat, divided surfaces; familiar controls; an explicit identity, key-custody, and host anchor. Frontmatter describes observed reusable values; `design-tokens.json` provides a portable export and `.impeccable/design.json` adds motion, layout, and preview snippets.

## Colors

**Primary.** Evergreen (`rail`) carries application navigation and primary actions. Pale leaf (`leaf`) marks current navigation and unread emphasis. White on evergreen is the main action treatment. The suite switcher uses the quieter `suite-selected` ground.

**Neutral.** Cool `canvas` separates white `surface` reading areas; `text` and `muted` establish hierarchy. `line` divides content, while the stronger `input` border defines fields and secondary actions. The rail has its own light foreground.

**Semantic.** Amber with brown ink communicates host/recovery warnings. Red appears with explicit error text and a pale error surface. Lavender labels private scope; pale incoming and leaf-tinted outgoing bubbles distinguish message ownership. Labels and shapes carry meaning alongside color.

**The Scope Rule.** Green selection never substitutes for public/private labels, key custody, or host scope. Keep those facts in words.

## Typography

Use the selected native UI sans stack from frontmatter; do not add a web font. Identifiers use the system monospace stack, wrap anywhere, and remain selectable. Weight synthesis is enabled in the prototype.

Page and form titles use `page` and `form-title`; section headings use `section`. Reading copy uses `body`, posts use `post`, and metadata stays subordinate. Names precede handles, timestamps, and protocol identifiers. Introductory copy is bounded at (65ch), posts at (70ch), and profile text at (60ch).

At mobile width (650px and below), body/post copy is (15px), page titles (26px), form titles (29px), and section headings (18px). Field text becomes (16px). Keep these observed overrides when extending the prototype.

## Layout

The desktop shell has an evergreen rail (232px) and a fluid stage. A white suite bar has minimum height (76px). Content is centered with maximum width (1500px), padding (28px 34px 48px), and a main/context grid with a context column (300px) and gap (28px). Forms center within (620px); narrower task content caps at (680px).

- At (1600px and above), content padding becomes (34px 48px); the centered task/context columns use a main cap (760px) and context range (260–320px).
- At (1200px and below), context is hidden, the workspace becomes one column, content padding is (26px), and the inbox list narrows from (300px) to (255px). Essential account details have a dedicated main-column treatment where provided.
- At (900px and below), the rail narrows to (180px), content padding is (24px), and the inbox shows its list alone; a dedicated conversation screen carries the thread.
- At (650px and below), the rail becomes a compact header with horizontal, scrollable app navigation. The suite switcher stays visible. Content padding is (22px 16px 35px). A separate mobile identity/host strip preserves custody context when the desktop anchor is hidden. Messages widen from (84%) to (92%); the graph shortens from (400px) to (300px).

Spacing is deliberately practical rather than a strict four-pixel grid: use the exported steps, plus component-specific values already present. Let action groups wrap, keep fluid children shrinkable, allow tables to scroll, and wrap long host URLs and identities. Keep the prototype/synthetic-data notice readable across widths.

## Elevation & Depth

Surfaces are flat and separated by tone and thin borders. There are no decorative card shadows. Only the transient status toast lifts above content with `0 8px 24px #153e3826`; its stacking level is (20), beneath the focused skip link (30).

Button and navigation color feedback runs for (150ms ease-out); the toast enters over (170ms ease-out) with a vertical movement (5px). Both are enabled only when reduced motion is not requested. Skeletons remain static.

## Shapes

Controls and navigation use gently curved corners; fields are slightly tighter; reading panels are slightly softer. The frontmatter records these distinct values rather than rounding everything uniformly. Avatars are circles (40px), with large profile variants (64px desktop; 54px mobile). Message bubbles use surface-size corners with one tight trailing corner (2px) to indicate ownership. Reply branches use a quiet vertical divider, not nested ornamental cards.

## Components

- **Buttons:** primary evergreen, secondary white with a strong border, transparent ghost, and red-text danger variants. Standard height is at least (40px), small actions (32px); mobile raises these to (44px) and (40px). Disabled controls use opacity (.55). Text actions remain underlined; hover changes background without shifting layout.
- **Navigation:** the rail uses leaf selection and `aria-current="page"`; suite selection uses a quieter pale ground. Rail targets are at least (46px) on desktop and (40px) on mobile. Preserve distinct application and suite navigation labels.
- **Fields:** visible labels, strong single-pixel borders, helpful text, and text errors. Padding is recorded in frontmatter; text areas start at (150px) and resize vertically. Keep native input behavior, password semantics, and validation.
- **Panels and lists:** white surfaces, single-pixel dividers, and the surface radius. Panels use (22px) internal padding, reduced to (19px) on mobile. Feed rows share one bounded container; avoid a separate floating card for every post.
- **Badges and notices:** compact text explains public/private, unread, and custody state. Notices pair status copy with a concrete action; errors use text as well as red. Never imply successful contact with a host from local preview feedback.
- **Identity/host anchor:** keep identity, key ownership, and host together. Reed/Loomed say keys are on this device; hosted accounts say this host holds the keys; Weft says read-only and names the observed host. App switching changes destination, not account ownership.
- **Conversations:** name and context above grouped messages, visible timestamps, an explicit private label, and a persistent composer. Mobile inbox opens the dedicated thread with a Back action.
- **Topology and metrics:** pair the graph with a readable node list and inspection controls. Distinguish nodes using labels and shapes as well as fill. Counts describe the configured host’s public view; never imply global coverage.

**The Visible Focus Rule.** Preserve the (3px) `focus` outline with (3px) offset on buttons, links, inputs, text areas, and selects. Keep the skip link, logical headings, labeled fields, pressed/current states, graph description, and polite status announcements. The prototype demonstrates these patterns; this document does not certify complete WCAG conformance.

## Do's and Don'ts

- **Do** preserve reading measure, visible focus, wrapping identifiers, and the mobile custody strip.
- **Do** label synthetic data and simulated actions; preserve public/private and read-only distinctions.
- **Do** retain local versus hosted key ownership and explain the actual host being observed or used.
- **Don't** add decorative shadows, animated loading pulses, new typefaces, or competing accent families.
- **Don't** infer shared login, key transfer, global network totals, delivery guarantees, or working backend actions from this prototype.
- **Don't** treat this package as an implemented production redesign. Production integration is a separate task.
