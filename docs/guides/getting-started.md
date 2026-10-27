---
title: Getting started
slug: getting-started
description: Run a local knot or provider and find the reference apps and checks.
order: 1
---

# Getting started

Run these commands from the repository root. The workspace uses Rust edition
2021; install a compatible stable Rust toolchain. `just` is an optional recipe runner.
Use Node.js 22 or later and npm for the documentation site.

## Run a self-hosted knot

```sh
cargo run -p tangled-knot -- --listen 127.0.0.1:3000 --data ./.data/local-knot
```

Keep the process running and inspect its advertised profiles:

```sh
curl http://127.0.0.1:3000/xrpc/tangled.node.describe
```

This starts a local host, not a public network. Keep its data directory to retain
state. The [knot README](../../crates/tangled-knot/README.md) documents its flags,
key handling, HTTP API, and gateway recipes.

## Run the provider

```sh
just provider
```

Open `http://127.0.0.1:8787` to create an account. This process holds account keys
and advertises `custodialSign: true`. Its default `data-provider/` directory holds
account and repository state. The provider's custody includes access to imported
private archive content.

The [provider README](../../crates/tangled-provider/README.md) documents `DATA_DIR`,
`PORT`, `PUBLIC_URL`, secure cookies, and account moves. The root
[Render blueprint](../../render.yaml) configures a persistent provider and static
browser apps. Deployment availability and successful third-party account login
must be verified separately.

## Run the apps

With a local knot running, use `just reed`, `just loomed`, or `just weft` for the
desktop clients. Desktop builds need the native dependencies required by Dioxus.
The `just reed-demo`, `just loomed-demo`, and `just weft-demo` recipes seed local
examples; inspect the [recipes](../../justfile) for lifecycle and port details.

Browser builds use `TANGLED_KNOT` as their configured default host. Desktop builds
default to `http://127.0.0.1:3000`. A seeded demo does not prove cross-knot pull,
multi-member circle interoperability, or clean-machine acceptance.

## Import existing activity

Sign into the provider and open **Your archive & connections**. Follow
[Social imports](../features/social-imports.md) for export uploads, privacy, recovery, and
host migration. [Live connectors](../features/connectors.md) run separately and require
the account owner's source authorization. Creating a destination connection does
not start source synchronization. WhatsApp support is experimental.

## Verify an implementation

```sh
cargo test --workspace
```

This excludes tests marked ignored. Live knot and SNS (Solana Name Service)
checks have additional setup documented in the
[client README](../../crates/tangled/README.md). The
[conformance chapter](../spec/02-conformance.md) maps advertised profiles to
required methods and [vectors](../spec/vectors/README.md).

To regenerate deterministic fixtures:

```sh
cd docs/spec/vectors
npm install
node generate.mjs
```

The MLS wrapping fixture contains synthetic payloads. Real RFC 9420 payloads are
covered by OpenMLS-backed tests, and are not expected to equal that fixture byte
for byte. Passing repository tests does not establish external service coverage
or certify an independently deployed knot.
