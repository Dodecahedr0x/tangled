# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Two jobs, one site:

1. A first-time visitor deciding whether Tangled is worth reading: they need to understand the protocol in a minute, see that Reed, Loomed, and Weft are applications on it (not the network itself), and reach the spec. They arrive at `/`.
2. Protocol implementers and researchers reading the specification: they need to implement a knot, a client, or a gateway, or to compare Tangled with ActivityPub, Nostr, AT Protocol, and Farcaster. They work in a code editor and a browser, often at night, with the spec and test vectors side by side. They live in `/docs` and `/spec`.

## Product Purpose

This repository is the Tangled protocol specification and a static public site. The site presents the project (landing) and hosts the implementable documents (docs). It is not a live network. Success is: a competent stranger can say what Tangled is and open the spec; a competent engineer can implement version 1 from the documents, CDDL, OpenAPI, and test vectors without asking the authors what a record is.

## Positioning

Tangled is a content substrate whose identity is a Solana key, whose store is a signed Merkle repository, and whose private data uses capabilities for fetch and encryption for read — on one data plane. Neighboring protocol docs cannot truthfully claim that combination. Reed (public social), Loomed (sealed circles), and Weft (read-only explorer) are applications on that plane; they are not hosted products and this site does not run them.

## Operating Context

Visitors meet a type-led landing, then enter the spec shell. Readers move from an architecture claim to a type page (Record, Capability, Circle) to an HTTP method to a test vector. They search. They copy DAG-CBOR examples. They use the playground to see a Solana address, a signature envelope, a capability chain, and a circle that keeps its id across the MLS threshold.

## Capabilities and Constraints

- Canonical spec lives in `spec/` markdown, CDDL, schemas, OpenAPI, interop tables, and vectors.
- Site is static (Astro). No backend. Playgrounds are client-only and labeled synthetic.
- Existing spec and reference URLs stay (`/spec/*`, `/reference/*`, `/terminology`, `/openapi.yaml`, `/llms.txt`).
- Every acronym is defined in Terminology.
- Visual commitment (user): rustdocs × Mintlify interaction grammar for docs, not Mintlify SaaS, not default Starlight. Landing inherits that world at a larger type scale; it is not a second brand.
- Do not claim app stores, production users, or a public graph to join.

## Brand Commitments

- Name: Tangled.
- Voice: RFC 2119, precise, honest about losses (no incumbent graph; AP imports are attested).
- Sit alongside docs.rs and Mintlify API docs for the reading instrument; the landing is still that instrument, scaled, not a SaaS marketing skin.
- Apps in this repository: Reed, Loomed, Weft.

## Evidence on Hand

- Protocol design in `spec/`.
- Application crates in `apps/reed`, `apps/loomed`, `apps/weft`.
- No production network, customers, or benchmarks. Do not fabricate them.

## Product Principles

1. Spec is the product; the site presents it, then lets you read it.
2. One data plane: public, cap, sealed.
3. Users never see HPKE vs MLS.
4. Do not claim a live network.
5. Define every acronym.

## Accessibility & Inclusion

WCAG 2.2 AA as a floor: skip link, heading order, contrast, keyboard nav, reduced-motion respect.
