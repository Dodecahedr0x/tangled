---
title: Apps and host configuration
slug: apps
description: The roles of Reed, Loomed, and Weft and how they connect to a knot.
order: 4
---

# Apps and host configuration

Reed, Loomed, and Weft use the same knot protocol. Each presents a different view
of signed repository data. App collections and interface choices sit above the
core protocol; an unfamiliar app collection can still be stored and synchronized.

## Reed

Reed presents public posts, profiles, follows, and a timeline. Following another
identity provides a reason to fetch its public repository. Posts appear from
received data; the existence of a follow record alone does not prove a successful
pull. Public social activity is separate from a person's private import archive.

The ActivityPub gateway is implemented by the knot. Reed follows foreign
identities through that facade rather than becoming a Mastodon REST client. See
[Interoperability](./interop.md) for provenance and the implementation boundary.

## Loomed

Loomed presents sealed conversations and circles. Access to the repository and
possession of the appropriate keys are separate requirements. A conversation
interface should show membership and message state rather than asking the person
to choose an encryption suite.

The private account archive is a different view. Loomed links to it from settings;
imported messages do not automatically become native circle conversations.

## Weft

Weft is a read-only public explorer. Its counts, topology, and lookup describe the
knots it observes, not an authoritative global directory. The knot's public stats
exclude private record bodies and learned peer locations; advertised peers are
operator-configured public URLs.

An empty result means this observer has no result. It does not establish that the
network contains no matching record. Optional loom and backlink results are
incomplete overlays, as defined in the specification.

## Selecting a host

Desktop builds default to `http://127.0.0.1:3000`. Browser builds use the
`TANGLED_KNOT` value baked into the build as their configured default. The Render
blueprint supplies the provider's public URL to each static browser app.

The landing page's `PUBLIC_PROVIDER_URL` controls the account link on the
separate documentation site. It does not configure a browser app's knot. Keeping
those settings distinct avoids a landing page that points to one provider while
an app requests another host.

Use [Getting started](../guides/getting-started.md) for desktop commands and local
demo recipes. A seeded demo supplies example content; it does not certify
cross-knot behavior or production readiness.

## Shared archive and protocol contract

Reed and Loomed settings link to the host's authenticated `/imports` archive.
Read [Social imports](./social-imports.md) for source data, identity preservation,
coverage warnings, recovery, and migration. Importing does not post back to the
source service or publish the archive to a public timeline.

[Use cases](../spec/20-use-cases.md) connects applications to the protocol data
model. Core requirements remain in [Architecture](../spec/03-architecture.md) and
[Conformance](../spec/02-conformance.md).

## Independently built apps

Reed, Loomed and Weft are initial clients rather than an exclusive application set. Personal workflows expose signed records and extensible collection schemas through the same repository data plane. Any app can retain unfamiliar collections, interpret the published profiles, preserve extensions, and use stable Tangled URIs and exact-revision CIDs. The reusable personal client is one implementation of those workflows; it is not a separate network or an application-private source of truth. See [A node for one person](./personal.md) for working commands, generic record access, and first-version limits.
