---
title: Feature overview
slug: index
description: Detailed documentation for identity, storage, synchronization, privacy, apps, and imports.
order: 0
---

# Feature documentation

These docs explain how Tangled's features behave, how they fit together, and
where the reference implementation has limits. For a high-level introduction or
first run, use the [guides](../guides/index.md). For exact wire formats and
normative requirements, use the [specs](../spec/00-abstract.md).

## Core features

| Feature | What these docs explain | Protocol contract |
| --- | --- | --- |
| [Hosting and identity](./hosting-and-identity.md) | Key custody, handles, account storage, and host moves | [Identifiers](../spec/04-identifiers.md), [migration](../spec/17-migration.md) |
| [Records and sync](./records-and-sync.md) | Records, blobs, commits, incremental pull, and failure states | [Data model](../spec/06-data-model.md), [sync](../spec/11-sync.md) |
| [Privacy and circles](./privacy-and-circles.md) | Fetch authorization, encrypted content, group keys, and metadata | [Visibility](../spec/08-visibility.md), [circles](../spec/10-circles.md) |
| [Apps](./apps.md) | Reed, Loomed, Weft, host selection, and the shared archive | [Use cases](../spec/20-use-cases.md) |
| [Social imports](./social-imports.md) | Archive formats, reconciliation, browsing, recovery, and migration | Application feature; distinct from public protocol bridges |
| [Live connectors](./connectors.md) | Source sign-in, ingest tokens, checkpoints, and coverage limits | Account-scoped archive ingestion |
| [Interoperability](./interop.md) | Public gateways, provenance, and canonical translation tables | [Interoperability](../spec/16-interoperability.md) |

## Read requirements and implementation status separately

A specification requirement describes what a conforming implementation must do.
A feature page describes behavior and operating boundaries of the reference
programs. Example data, seeded demos, and advertised profiles are useful evidence,
but are not independent conformance certification.

The [HTTP reference](/reference/http), [type index](/reference/types), and
[OpenAPI](/reference/openapi) support implementation work. The JSON examples and
type summaries are reading aids; the formal contract lives in the spec chapters,
CDDL, schemas, and OpenAPI under `docs/spec/`.

## Common implementation boundaries

- The provider holds account keys and can read that account's private archive.
- Private social imports do not become native multi-member circle conversations.
- Public proofs expose some structural metadata even when bodies are sealed.
- Foreign-service permissions and API access bound import coverage; WhatsApp
  support is experimental.
- Hosting a provider or running a demo does not establish a production network.

Use the relevant feature page for precise limits before choosing a hosting or
integration approach.
