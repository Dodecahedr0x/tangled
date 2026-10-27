# Go to market

Internal strategy, updated 2026-10-27. This document is not published as a guide.
It describes release gates rather than asserting those gates have passed.

## Current offer

Tangled combines a protocol specification, Rust reference implementations, three
apps, and a hosted-provider implementation. A person can self-host or create an
account on an operated provider. The provider generates the identity and holds
its keys; `custodialSign: true` discloses that custody.

The static documentation site shows an account link only when
`PUBLIC_PROVIDER_URL` is configured. Reed, Loomed, and Weft can be deployed as
static browser apps pointing at the provider through the Render blueprint.
Desktop builds default to a local knot. The docs site itself does not run them.

No production user count, incumbent graph, uptime result, or independent
conformance certification is established by the repository.

## Audiences and next steps

| Reader | Useful next step | Evidence of success |
| --- | --- | --- |
| First-time visitor | Understand the hosting choice; open an app or a configured provider | Can explain where their account keys live |
| Curious reader | Read the mechanism map and comparison | Can name identity, knot, record, visibility, circle, and pull |
| Self-hoster | Follow getting-started commands and local demos | A reproducible local run |
| Protocol implementer | Read conformance, API, CDDL, and vectors | Publishes advertised profiles and results |

## Release gates

1. Keep guides and normative specifications in `docs/`; generate the site from
   those sources. Clearly distinguish protocol requirements from implementation
   status and planned acceptance work.
2. Validate two knots on a clean machine: a public post crosses by pull, a sealed
   conversation is readable only by authorized participants, and Weft exposes
   only the public side. A seeded single-knot demo does not meet this gate.
3. Publish a walkthrough and release note only after that validation. The
   cluster acceptance design under `../superpowers/specs/` remains a plan until
   there is an executable result.
4. Before claiming outside conformance, obtain an implementation's advertised
   profiles and vector results. Public HTTP alone cannot certify sealed or group
   behavior. A standalone conformance checker is still a proposed deliverable.
5. Validate source-account login and actual history/media coverage with account
   owners before describing live imports as production-ready. WhatsApp remains
   experimental; platform access and source permissions bound coverage.

## Communication constraints

State the custody model where an account is offered. Explain the difference
between fetch authorization and decryption. Preserve the limits: private data
cannot be globally indexed, revocation cannot recall fetched bytes, handles rely
on DNS or SNS, and unsigned ActivityPub imports are gateway-attested.

Distribution can use the site, repository, a reproducible walkthrough, and later
an implementer note. Do not claim public adoption, a global directory, or hosted
availability from an undeployed blueprint. No outbound campaign is authorized by
this document.

The canonical reader-facing explanations live in `../guides/index.md`,
`../guides/getting-started.md`, `../features/social-imports.md`, and `../spec/`. Maintain those
sources rather than keeping competing drafts of landing copy and walkthroughs
here.
