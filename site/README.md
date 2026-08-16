# Site

Static Astro app: landing at `/`, docs (the spec) at `/docs`. Renders `../spec/`.

```bash
npm install
npm run dev    # http://localhost:4321
npm run build  # dist/ is fully static; Pagefind index included
```

CI deploys `dist/` to GitHub Pages (`https://dodecahedr0x.github.io/tangled/`) on push to `main`. Local preview stays at `/`. Set `ASTRO_BASE=/tangled` to mimic Pages.

Visual contract: `../DESIGN.md`. Product: `../PRODUCT.md`.
