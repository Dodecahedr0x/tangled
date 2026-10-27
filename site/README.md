# Site

Static Astro app: landing at `/`, guides at `/guides`, detailed feature docs at
`/docs`, and formal specs at `/spec`.
All documentation sources live in [`../docs/`](../docs/README.md): Markdown guides,
protocol chapters, machine-readable companions, and curated reference data.
Astro owns the shell and interactive tools. See the documentation README for
authoring, Mermaid, URL mapping, and publication boundaries.

```bash
npm ci
npm test
npm run dev    # http://localhost:4321
npm run build  # dist/ is fully static; Pagefind index included
npm run check:docs # validate built links, fragments, and raw companions
```

CI deploys `dist/` to GitHub Pages (`https://dodecahedr0x.github.io/tangled/`) on push to `main`. Local preview stays at `/`. Set `ASTRO_BASE=/tangled` to mimic Pages.

Visual contract: `../DESIGN.md`. Product: `../PRODUCT.md`.
