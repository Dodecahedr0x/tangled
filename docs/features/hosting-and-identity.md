---
title: Hosting and identity
slug: hosting-and-identity
description: How identity keys, handles, provider custody, and host migration work together.
order: 1
---

# Hosting and identity

An identity names a signed repository. A knot stores that repository and its
blobs. Separating those roles lets a repository move while its identity remains
stable. See [Getting started](../guides/getting-started.md) for launch commands.

## Identity keys and human handles

The protocol's identity is a Solana Ed25519 public key expressed as a `did:sol`.
Creating a generative identity does not require an on-chain registration. The
repository's identity record describes operational keys and endpoints. Key
rotation changes those operational keys according to the protocol's rotation and
recovery rules; it is separate from changing the machine that hosts the bytes.

A DNS or Solana Name Service (SNS) handle is a human-readable pointer. Resolution
checks the handle binding and the identity's advertised names. A readable name is
not the authentication primitive, and a first local run does not need one.

## Self-hosted knot

The knot creates its local identity in `{data}/identity.json` on first run. Its
data directory also retains repository state and blobs. Keep that directory
private and persistent; starting with a new empty directory creates new local
state rather than recovering the previous identity.

`--listen` selects the bind address. `--public-url` sets the advertised endpoint.
They are different settings: an externally reachable HTTPS endpoint can sit in
front of a loopback-bound process. `--peer` and `TANGLED_PEERS` configure publicly
advertised peer URLs. Learned peer locations stay local.

The [knot README](../../crates/tangled-knot/README.md) lists flags and the reference
server's implementation status. The host is responsible for availability; a
signature authenticates history but does not force a host to retain or serve it.

## Hosted provider

The provider creates accounts from an email, name, and password. It generates and
stores account keys and advertises `custodialSign: true`. It can sign for an
account and decrypt its imported archive. Encrypting the repository's stored
bodies does not remove that custody.

`DATA_DIR` holds account and repository state. `PORT` selects the local port;
`PUBLIC_URL` or `RENDER_EXTERNAL_URL` supplies the advertised URL. The provider
uses secure cookies for HTTPS public URLs. The Render blueprint attaches
persistent storage to a single provider instance; it does not supply a global
network or an availability guarantee.

## Moving and recovering

A location-only provider move preserves the account keys and DID. The destination
receives custody and private archive material through an encrypted handoff. The
source fences writes and connector ingestion, and retires custody only after the
destination acknowledges durable, verified receipt. An interrupted acknowledgement
can be resumed with the same move details and destination password.

Source-service sessions and connector tokens do not migrate. Reauthorize helpers
on the destination. Moving hosts is also different from restoring a recovery
file: [Social imports](./social-imports.md#back-up-and-recover-without-a-provider)
explains the passphrase-protected backup and its sensitive contents.

## Protocol contract

Read [Identifiers](../spec/04-identifiers.md),
[Resolution](../spec/13-resolution.md), and
[Migration and multi-device](../spec/17-migration.md) for exact record fields,
verification, rotation, and recovery rules.
