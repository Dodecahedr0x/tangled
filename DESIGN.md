---
name: Tangled
description: Protocol site — rustdocs type index × Mintlify API grammar, landing as stacked courses
colors:
  bg: "#12141a"
  bg-sidebar: "#0c0e12"
  bg-code: "#1a1d24"
  bg-elevated: "#181b22"
  text: "#e6e8ee"
  text-muted: "#9aa3ad"
  text-faint: "#6b7380"
  accent: "#c47a4a"
  accent-hover: "#d4915c"
  accent-dim: "#3d2a1f"
  border: "#2a2f38"
  border-strong: "#3d4450"
  method-get: "#5b9a6d"
  method-post: "#c47a4a"
  danger: "#c45c4a"
  callout-note: "#3d2a1f"
  scrim: "rgba(0, 0, 0, 0.55)"
typography:
  ui:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 450
    lineHeight: 1.45
    letterSpacing: "0.01em"
  body:
    fontFamily: "Source Serif 4, Iowan Old Style, Palatino, Georgia, serif"
    fontSize: "17px"
    fontWeight: 400
    lineHeight: 1.65
    letterSpacing: "0"
  display:
    fontFamily: "Source Serif 4, Iowan Old Style, Palatino, Georgia, serif"
    fontSize: "clamp(2.75rem, 7vw, 5.5rem)"
    fontWeight: 400
    lineHeight: 1.05
    letterSpacing: "-0.03em"
  heading:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(1.75rem, 3vw, 2.25rem)"
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: "-0.02em"
  lead:
    fontFamily: "Source Serif 4, Iowan Old Style, Palatino, Georgia, serif"
    fontSize: "1.25rem"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "0"
  course:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(1.5rem, 3vw, 2rem)"
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: "-0.02em"
  app-name:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "0"
  chrome:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "0.04em"
  kbd:
    fontFamily: "Source Code Pro, ui-monospace, Menlo, monospace"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.2
    letterSpacing: "0"
  toc:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: "0"
  foot:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "0"
  play-title:
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 500
    lineHeight: 1.3
    letterSpacing: "0"
  mono:
    fontFamily: "Source Code Pro, ui-monospace, Menlo, monospace"
    fontSize: "13.5px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "0"
rounded:
  sm: "2px"
  md: "4px"
spacing:
  sm: "8px"
  md: "16px"
  lg: "32px"
  xl: "80px"
  sidebar: "272px"
  toc: "220px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.bg}"
    rounded: "{rounded.sm}"
    padding: "10px 18px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: "10px 18px"
  nav-active:
    backgroundColor: "{colors.accent-dim}"
    textColor: "{colors.accent}"
    rounded: "{rounded.sm}"
    padding: "6px 10px"
---

# Design

## Overview

Dark-first specification instrument. Rustdoc’s type density and module tree; Mintlify’s left nav, right TOC, method pages, and callouts. Copper accent on slate. Not a SaaS marketing docs theme and not default Starlight.

Landing (`/`) is the same world at architectural scale: stacked horizontal courses labeled as rustdoc paths, type-led, no card grid. Docs (`/docs`) stay the dense reading shell.

THESIS: The spec is a type-indexed instrument; the landing is a vertical rise through its courses. The category-default centered hero with gradient cards is refused.

OWN-WORLD: Slate `#12141a`, sidebar `#0c0e12`, copper `#c47a4a`, Fira Sans UI, Source Serif 4 body and landing display, Source Code Pro signatures. Hairline borders, 2px radii, method-color ticks.

STORY: A visitor understands the substrate, sees Reed / Loomed / Weft as apps on it, and enters the spec. An implementer finds the claim, the type, the HTTP method, and the vector without leaving the shell.

FIRST VIEWPORT (landing): Full-bleed slate; 48px chrome with Landing | Docs | Reference; one copper kicker `tangled::protocol`; monumental serif line; one paragraph; Read the spec / GitHub; honest “not a running network.” No diagram, no cards.

FORM: Surface courses (seed `dea6db17`, assigned index 7), restrained color. Dark because the scene is night-time spec reading. Comp B plates.

## Colors

Restrained: neutrals plus one copper accent. Accent owns the active nav, links, primary CTA, and focus rings — not decorative glows. Syntax highlighting stays inside the code surface. Light theme is provided but secondary: invert to `#f4f1ea` paper with `#1c1916` text and the same copper.

## Typography

Fira Sans for chrome and docs headings (rustdoc UI). Source Serif 4 for chapter prose and for the landing display line. Source Code Pro for types, CIDs, HTTP paths, and course kickers (`tangled::knot`). Docs type pages lead with a signature line in mono, rustdoc-style. Landing display: weight 400, tracking about `-0.03em`, measure of the lead ≤ 32em.

## Layout

Shared chrome: skip link, 48px top bar (wordmark, Landing / Docs / Reference, theme, GitHub). Search lives in the docs top bar only.

Docs shell: 272px left nav, main, 220px TOC above 1100px. Docs home (`/docs`) is a claims board plus architecture figure.

Landing: no sidebar. Stacked courses with more space above a heading than below it (course padding ~104px / 48px). The first course is shorter than the viewport so the next rustdoc kicker peeks. Apps are a hairline row list (`apps::reed`), not equal cards. Close the page with a real spec CTA, not a newsletter. Landing chrome uses page `--bg`, not the docs sidebar strip.

## Elevation & Depth

No drop shadows. Separation is 1px `{colors.border}` and background steps (`bg` → `bg-elevated` → `bg-code`).

## Shapes

2px radius. Callouts are left-bordered blocks, not pills. Method badges are square ticks of color. Primary button is a copper rectangle, not a pill.

## Components

- Site nav: three items. Docs is active on spec, terminology, playground, compare, interop. Reference is active under `/reference`.
- Sidebar: `spec::identity` module grouping, rustdoc path style, plus Docs overview.
- Callouts: Note / Must / Warning with copper or danger left bar.
- Search: Pagefind overlay, keyboard `/`, docs only.
- Playground: labeled **synthetic**; monospace I/O.
- Landing CTA: copper fill, ghost hairline; `:active` scale 0.96.

## Do's and Don'ts

- Do keep copper scarce.
- Do expand acronyms on first use; link Terminology.
- Don’t use neon, glassmorphism, or gradient heroes.
- Don’t imply a live network or hosted apps.
- Don’t expose HPKE vs MLS as a user control in the playground.
- Don’t structure the landing as a trio of icon cards.
