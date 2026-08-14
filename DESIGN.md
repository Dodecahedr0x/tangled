---
name: Tangled
description: Protocol specification site — rustdocs type index × Mintlify API grammar
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
    fontFamily: "Fira Sans, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(1.75rem, 3vw, 2.25rem)"
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: "-0.02em"
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
  sidebar: "272px"
  toc: "220px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.bg}"
    rounded: "{rounded.sm}"
    padding: "8px 14px"
  nav-active:
    backgroundColor: "{colors.accent-dim}"
    textColor: "{colors.accent}"
    rounded: "{rounded.sm}"
    padding: "6px 10px"
---

# Design

## Overview

Dark-first specification instrument. Rustdoc’s type density and module tree; Mintlify’s left nav, right TOC, method pages, and callouts. Copper accent on slate. Not a SaaS marketing docs theme and not default Starlight.

THESIS: The spec is a type-indexed instrument. The category-default centered hero with gradient cards is refused.

OWN-WORLD: Slate `#12141a`, sidebar `#0c0e12`, copper `#c47a4a`, Fira Sans UI, Source Serif 4 body, Source Code Pro signatures. Hairline borders, 2px radii, method-color ticks.

STORY: An implementer finds the claim, the type, the HTTP method, and the vector without leaving the shell.

FIRST VIEWPORT: Full-height dark shell; 272px left rail with module tree; main column opens on claims + architecture diagram; search in the top bar; no marketing hero.

FORM: rustdocs × Mintlify canon (user-pinned). Restrained color. Dark because the scene is night-time spec reading.

## Colors

Restrained: neutrals plus one copper accent. Accent owns the active nav, links, and focus rings — not decorative glows. Syntax highlighting stays inside the code surface. Light theme is provided but secondary: invert to `#f4f1ea` paper with `#1c1916` text and the same copper.

## Typography

Fira Sans for chrome (rustdoc UI). Source Serif 4 for chapter prose (rustdoc body). Source Code Pro for types, CIDs, and HTTP paths. Type pages lead with a signature line in mono, rustdoc-style.

## Layout

App shell: skip link, 48px top bar (wordmark, search, theme), 272px left nav, main, 220px TOC above 1100px. Home is a claims board plus mermaid, not a hero. Type pages: signature, fields table, impl notes. HTTP pages: method badge, path, params, example req/res.

## Elevation & Depth

No drop shadows. Separation is 1px `{colors.border}` and background steps (`bg` → `bg-elevated` → `bg-code`).

## Shapes

2px radius. Callouts are left-bordered blocks, not pills. Method badges are square ticks of color.

## Components

- Sidebar: `spec::identity` module grouping, rustdoc path style.
- Callouts: Note / Must / Warning with copper or danger left bar.
- Search: Pagefind overlay, keyboard `/`.
- Playground: labeled **synthetic**; monospace I/O.

## Do's and Don'ts

- Do keep copper scarce.
- Do expand acronyms on first use; link Terminology.
- Don’t use neon, glassmorphism, or gradient heroes.
- Don’t imply a live network.
- Don’t expose HPKE vs MLS as a user control in the playground.
