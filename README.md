# Tangled

A content-network protocol: Solana-key identity, signed Merkle repositories, capability authorization, sealed records, and normative bridges to other networks.

This repository is the **implementable specification** and a **static documentation site**. It is not a live network and does not run knots.

- Canonical spec: [`spec/`](spec/)
- Site: [`site/`](site/)
- Product / visual contracts: [`PRODUCT.md`](PRODUCT.md), [`DESIGN.md`](DESIGN.md)

## Read the spec

```
spec/00-abstract.md … spec/19-comparison.md
spec/cddl/          DAG-CBOR types
spec/schemas/       JSON Schema for core and example collections
spec/http/openapi.yaml
spec/interop/translations.yaml
spec/vectors/       cryptographic fixtures
```

## Docs site

```bash
cd site
npm install
npm run dev
```

Build is fully static (`npm run build`). Search is Pagefind, generated at build time.

## Vectors

```bash
cd spec/vectors
npm install
node generate.mjs
```

Seed is `BLAKE3("tangled-vector-seed-v1")`. MLS payloads in the fixtures are synthetic stand-ins for RFC 9420 messages; the Tangled wrapping is normative.

## License

Specification text and site: Apache-2.0, unless a file says otherwise.
