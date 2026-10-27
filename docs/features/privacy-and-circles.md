---
title: Privacy and circles
slug: privacy-and-circles
description: Fetch permissions, encryption, group membership, and the privacy boundaries of a shared repository.
order: 3
---

# Privacy and circles

Tangled separates permission to fetch bytes from the ability to read their
contents. Both apply within one repository; a private message does not use a
second synchronization protocol.

## Three visibility classes

| Visibility | Who may fetch | Who may read |
| --- | --- | --- |
| `public` | Anyone | Anyone |
| `cap` | A holder of an authorizing capability | Anyone with the bytes, including the host |
| `sealed` | A holder of an authorizing capability | Holders of the decryption keys |

A capability is a scoped, expiring certificate. Its resource and command matter:
a grant for one collection does not authorize sibling collections, and a
repository read grant is not a circle membership grant. Revocation enforcement
uses expiry and online or cached revocation information. It cannot recall bytes
someone already received.

## Sealed bodies and attachments

Encryption protects the body from a host that lacks the decryption key. It does
not conceal all repository activity: public Merkle proofs can reveal opaque
sibling keys, CIDs, and tree structure. Treat sealed bodies as content protection,
not metadata anonymity.

Attachments need their own encrypted bytes. A blob referenced by a public record
is public even if a sealed record also references it. Access control on the
sealed record cannot undo that public reference.

A hosted provider holds account keys. It can therefore read an account's private
archive even though the stored bodies are encrypted. The
[hosting docs](./hosting-and-identity.md) explain that custody choice.

## Circle membership and epochs

A circle identifies a group and its evolving membership. Its identity remains
stable as people join and leave. Epoch secrets let members decrypt the content
for their authorized epochs; removing a member cannot make them forget plaintext
or keys they already received.

The protocol specifies Hybrid Public Key Encryption (HPKE) for small groups and
Messaging Layer Security (MLS) for larger groups. Size migration and membership
changes have exact ordering and compatibility rules. Applications should expose
people and messages, rather than an encryption-suite setting.

## Messaging and private archives are separate features

Loomed is the messenger client. The account's social-import archive instead uses
owner-only encrypted collections and its own local materialization. Creating an
archive connection does not create a native messaging circle or invite imported
senders to Tangled.

The reference crypto library includes OpenMLS-backed tests. Synthetic MLS
wrapping vectors describe wrapping fields and exporter parameters; they are not
byte-for-byte live RFC 9420 messages. Neither those fixtures nor a seeded app demo
establish independent multi-member messaging conformance.

## Protocol contract

Read [Visibility](../spec/08-visibility.md),
[Capabilities](../spec/09-capabilities.md),
[Circles](../spec/10-circles.md), and the
[Threat model](../spec/18-threat-model.md) for formal authorization, cryptography,
and residual-risk requirements.
