# Documentation source consolidation

**Goal:** Generate the site's documentation from `docs/`, include the full
specification and its machine-readable companions, and render Mermaid diagrams.

**Approach:** Keep the existing Astro shell and public URLs. Move `spec/` to
`docs/spec/`; load guides as an Astro collection; keep internal plans out of the
published collection. Move curated reference data into `docs/reference/`.

- [x] Move specification sources and update fixture consumers and source links.
- [x] Replace hard-coded guide prose with Markdown, add setup guidance, and
      reconcile provider, self-hosting, and implementation-status claims.
- [x] Generate guide navigation and raw downloads from the canonical sources.
- [x] Resolve relative Markdown links for both local and GitHub Pages builds.
- [x] Render Mermaid fences with the existing bundled dependency, with readable
      source fallback, theme support, and responsive overflow.
- [x] Verify static output, local links, raw assets, base paths, browser SVG
      rendering, and Rust vector consumers.

**Review focus:** Relative links and fragments; nested base URLs; stale copied
assets; accidental publication of internal plans; encrypted versus custodial
data claims. Preserve unrelated working-tree changes.

Verification: local and `/tangled` builds pass; 67 pages and 3,223 local links
validated in each build; all raw companions match their sources; 4 documentation
tests, 15 Rust vector tests, and the MLS wrapping fixture test pass. All five
diagrams render as SVG in the browser, including after a theme switch.
