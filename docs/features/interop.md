---
title: Interoperability
slug: interop
description: Canonical translation tables and provenance rules for protocol gateways.
order: 7
---

# Interoperability

The canonical mapping is [translations.yaml](../spec/interop/translations.yaml).
Gateways MUST preserve `bytes_cid` so an export can emit the original bytes.
Unsigned ActivityPub imports are attested by the gateway, not signed by the
original author. Read [Interoperability](../spec/16-interoperability.md) for the
requirements, losses, and round-trip rules.

The YAML is published directly from `docs/spec/interop/`; the site does not
maintain a second translation table.

## Private archive imports

Telegram, X, and WhatsApp archive imports are a separate account feature.
They preserve source identities and original evidence without publishing the
content through a public gateway. See [Social imports](./social-imports.md) and
[Live connectors](./connectors.md) for their privacy model and coverage limits.
