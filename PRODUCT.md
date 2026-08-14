# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Protocol implementers and researchers reading a specification: they need to implement a knot, a client, or a gateway, or to compare Tangled with ActivityPub, Nostr, AT Protocol, and Farcaster. They work in a code editor and a browser, often at night, with the spec and test vectors side by side.

## Product Purpose

This repository is the Tangled protocol specification and a static documentation site that presents it. It is not a live network. Success is: a competent engineer can implement version 1 from the documents, CDDL, OpenAPI, and test vectors without asking the authors what a record is.

## Positioning

Tangled is a content substrate whose identity is a Solana key, whose store is a signed Merkle repository, and whose private data uses capabilities for fetch and encryption for read — on one data plane. Neighboring protocol docs cannot truthfully claim that combination.

## Operating Context

Readers move from an architecture claim to a type page (Record, Capability, Circle) to an HTTP method to a test vector. They search. They copy DAG-CBOR examples. They use the playground to see a Solana address, a signature envelope, a capability chain, and a circle that keeps its id across the MLS threshold.

## Capabilities and Constraints

- Canonical spec lives in `spec/` markdown, CDDL, schemas, OpenAPI, interop tables, and vectors.
- Site is static (Astro). No backend. Playgrounds are client-only and labeled synthetic.
- Every acronym is defined in Terminology.
- Visual commitment (user): rustdocs × Mintlify interaction grammar, not Mintlify SaaS, not default Starlight.

## Brand Commitments

- Name: Tangled.
- Voice: RFC 2119, precise, honest about losses (no incumbent graph; AP imports are attested).
- Sit alongside docs.rs and Mintlify API docs.

## Evidence on Hand

- Protocol design in `spec/`.
- No production network, customers, or benchmarks. Do not fabricate them.

## Product Principles

1. Spec is the product; the site is a reading instrument.
2. One data plane: public, cap, sealed.
3. Users never see HPKE vs MLS.
4. Do not claim a live network.
5. Define every acronym.

## Accessibility & Inclusion

WCAG 2.2 AA as a floor: skip link, heading order, contrast, keyboard nav, reduced-motion respect.
