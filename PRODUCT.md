# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Three documentation levels follow the landing:

1. A first-time visitor who is not a protocol engineer: they need to understand Tangled in a minute, open Reed, Loomed, or Weft, and take one step. They arrive at `/`. Desktop builds still run on the person's own machine.
2. Readers start with high-level introductions and setup at `/guides`, then use `/docs` for detailed feature behavior, configuration, examples, and implementation boundaries.
3. Protocol implementers and researchers: they implement a knot, a client, or a gateway from the specification, CDDL, OpenAPI, and test vectors. They live in `/spec` and `/reference`.

## Product Purpose

This repository is the Tangled protocol specification and a static public site. The site is a funnel: landing → guides (introduction and setup) → docs (feature detail) → specs (formal protocol). It is not a live network. Success is: a stranger can say what Tangled is; a reader can name the pieces; an engineer can implement version 1 without asking the authors what a record is.

## Positioning

Tangled is a content substrate whose identity is a Solana key, whose store is a signed Merkle repository, and whose private data uses capabilities for fetch and encryption for read — on one data plane. Neighboring protocol docs cannot truthfully claim that combination. Reed (public social), Loomed (sealed circles), and Weft (read-only explorer) are applications on that plane. The Render blueprint hosts each as a static site whose default knot is the hosted provider. This documentation site does not run them. Desktop builds still default to a local knot.

## Operating Context

Visitors meet a plain-language landing, then introductory guides, detailed feature docs, and formal specs. Implementers move from an architecture claim to a type page (Record, Capability, Circle) to an HTTP method to a test vector.

## Capabilities and Constraints

- Guides live in `docs/guides/`; feature docs in `docs/features/`; formal protocol specs in `docs/spec/`. All three levels are generated from those sources.
- Canonical spec lives in `docs/spec/` markdown, CDDL, schemas, OpenAPI, interop tables, and vectors.
- Site is static (Astro). No backend. Playgrounds are client-only and labeled synthetic.
- Existing spec and reference URLs stay (`/spec/*`, `/reference/*`, `/terminology`, `/openapi.yaml`, `/llms.txt`).
- Every acronym is defined in Terminology.
- Visual commitment (user): rustdocs × Mintlify interaction grammar for docs and spec, not Mintlify SaaS, not default Starlight. Landing inherits that world at a larger type scale and uses ordinary words, not rustdoc path kickers.
- Do not claim app stores, production users, or a public graph to join.

## Brand Commitments

- Name: Tangled.
- Voice: RFC 2119, precise, honest about losses (no incumbent graph; AP imports are attested).
- Sit alongside docs.rs and Mintlify API docs for the reading instrument. The landing is still that world, scaled, with a funnel mouth in plain speech.
- Apps in this repository: Reed, Loomed, Weft.

## Evidence on Hand

- Protocol design in `docs/spec/`.
- Application crates in `apps/reed`, `apps/loomed`, `apps/weft`.
- No production network, customers, or benchmarks. Do not fabricate them.

## Hosted provider

`crates/tangled-provider` is one knot a person can join without running a process. Signup is email, name, and password. The provider generates the `did:sol` and holds the seeds. `node.describe` sets `custodialSign: true`. Render: `render.yaml` (paid web service, persistent disk at `/var/tangled`, plus static sites for Reed, Loomed, and Weft). Each app build sets `TANGLED_KNOT` from the provider's public URL. The documentation site links to the provider only when `PUBLIC_PROVIDER_URL` is set at build time. Self-hosting a knot stays valid. Do not invent user counts.

## Product Principles

Social imports add a private account archive at `/imports` for Telegram, Twitter/X, and WhatsApp exports plus browser-hosted source pairing and optional external live helpers. Imports include available received/group content, preserve source identities and original bytes, and do not republish to the public graph. Reed and Loomed settings link to the shared archive. Hosted imports are custodial and disclose provider visibility. Encrypted resumable uploads, owner-confirmed host-retained erasure, independent passphrase recovery, and destination-acknowledged migration cover the archive lifecycle. Live source-account compatibility remains unverified; WhatsApp is experimental. Exact coverage, setup, assumptions, and current limits are documented in `docs/features/social-imports.md`.

1. Funnel: landing → guides → docs → specs. The hosted account is the easy first step when `PUBLIC_PROVIDER_URL` is set.
2. One data plane: public, cap, sealed.
3. Users never see HPKE vs MLS.
4. Do not invent a public graph or user counts. A deployed provider is one knot, and it says it holds the keys.
5. Define every acronym.

## Accessibility & Inclusion

WCAG 2.2 AA as a floor: skip link, heading order, contrast, keyboard nav, reduced-motion respect.
