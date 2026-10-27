# Tangled

A content-network protocol: Solana-key identity, signed Merkle repositories, capability authorization, sealed records, and normative bridges to other networks.

This repository contains the **version 1 specification**, Rust reference implementations, Reed, Loomed, Weft, a hosted provider, and a **static documentation site**. The site explains the protocol; the provider and apps run separately. No public network size or production adoption is claimed.

- Documentation: [`docs/`](docs/) — guides, specifications, and reference sources
- Canonical spec: [`docs/spec/`](docs/spec/)
- Site: [`site/`](site/) — landing at `/`, guides at `/guides`, feature docs at `/docs`, specs at `/spec/…`
- Rust reference client: [`crates/tangled/`](crates/tangled/)
- Knot HTTP server: [`crates/tangled-knot/`](crates/tangled-knot/)
- Hosted provider (account signup, Render blueprint): [`crates/tangled-provider/`](crates/tangled-provider/) — `just provider`
- Loomed messenger: [`apps/loomed/`](apps/loomed/)
- Weft network explorer: [`apps/weft/`](apps/weft/)
- Social activity imports: [`docs/features/social-imports.md`](docs/features/social-imports.md) — private archives, Telegram/X/WhatsApp connector setup, recovery, and implementation assumptions.
- Product / visual contracts: [`PRODUCT.md`](PRODUCT.md), [`DESIGN.md`](DESIGN.md)

## Read the spec

```
docs/spec/00-abstract.md … docs/spec/21-changelog.md
docs/spec/cddl/          DAG-CBOR types
docs/spec/schemas/       JSON Schema for core and example collections
docs/spec/http/openapi.yaml
docs/spec/interop/translations.yaml
docs/spec/vectors/       cryptographic fixtures
```

## Site

Landing at `/`. Guides at `/guides` introduce Tangled and setup. Docs at `/docs` explain features. Specs at `/spec/…` define the formal protocol. Preview:

```bash
just docs
# or: cd site && npm install && npm run dev
```

Build is fully static (`npm run build` in `site/`). Search is Pagefind, generated at build time.

GitHub Pages: push to `main` publishes [dodecahedr0x.github.io/tangled](https://dodecahedr0x.github.io/tangled/). First time: repo **Settings → Pages → Source: GitHub Actions**.

## Vectors

```bash
cd docs/spec/vectors
npm install
node generate.mjs
```

Seed is `BLAKE3("tangled-vector-seed-v1")`. MLS payloads in `mls-welcome-commit.json` are synthetic stand-ins for RFC 9420 messages; the Tangled wrapping is normative. The Rust crates produce and verify real OpenMLS Welcome/Commit bytes in tests (not byte-equal to that fixture).

## Rust crates

```bash
cargo test --workspace
cargo run -p tangled -- identity
cargo run -p tangled-knot -- --listen 127.0.0.1:3000 --data ./data
```

Live SNS handle resolution (`getAccountInfo`) is skipped unless `SOLANA_RPC` is set. CI starts `solana-test-validator` (Agave **v4.2.1**) with fixtures; locally:

```bash
# loads crates/tangled/fixtures/sns (same as CI) — not a bare --reset
scripts/sns-validator.sh --bg
SOLANA_RPC=http://127.0.0.1:8899 cargo test -p tangled --test sns_rpc -- --ignored
```

See [`crates/tangled/README.md`](crates/tangled/README.md) (client) and [`crates/tangled-knot/README.md`](crates/tangled-knot/README.md) (knot server).

## License

Specification text and site: Apache-2.0, unless a file says otherwise.
