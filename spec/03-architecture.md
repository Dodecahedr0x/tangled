---
title: Architecture
slug: architecture
order: 3
---

# Architecture

Tangled separates **who you are**, **what you wrote**, **who may fetch it**, **who may read it**, and **who helps you find it**. Those are different jobs. Incumbent protocols fuse them, then pay for the fusion.

## Actors

```
  Wallet ──sign──► Knot A (repo + blobs)
                      ▲
                      │ Merkle pull
                      │
                   Knot B
                      │
                      ▼
              Loom (optional index of public heads)
                      │
              Gateway knot ──translate──► ActivityPub / Nostr / AT Protocol
```

**Identity** is a Solana Ed25519 public key. A wallet holds it. See [Identifiers](./04-identifiers.md).

**Knot** is the host. It stores one or more repositories and their blobs. It answers HTTP. It is cheap to self-host: one process, one local database (SQLite is RECOMMENDED, not required). A provider MAY host many identities on one knot. The knot is not the identity. If the knot lies, signatures and Merkle proofs detect it. If the knot disappears, the repository CAR moves to another knot.

**Repo** is one identity’s signed Merkle Search Tree (MST) plus blobs. The identity’s `sys/id` record lives here.

**Record** is a typed object in the MST, signed by a Solana key.

**Capability** is a certificate that authorizes a command on a resource. Knots that advertise `knot.cap` verify capabilities before serving non-public bytes.

**Circle** is a group. Members share an epoch secret. Users add and remove members. They do not pick Hybrid Public Key Encryption (HPKE) or Messaging Layer Security (MLS).

**Loom** indexes **public** heads. Clients choose looms. A loom can censor its index; it cannot rewrite a repository. Looms are outside the Trusted Computing Base (TCB).

**Gateway** is a knot that also speaks a foreign protocol. Translations are stored as `sys/foreign` records with provenance. Gateways are outside the TCB for authenticity of ActivityPub (AP) objects (those objects are not author-signed). For Nostr and AT Protocol, the original signatures remain verifiable.

## Data plane

One repository, one sync protocol, three visibility values on the record:

| `vis` | Fetch | Read | Rebroadcast |
| --- | --- | --- | --- |
| `public` | Anyone | Anyone | Yes |
| `cap` | Bearer of a valid capability | Anyone who fetched (host included) | No |
| `sealed` | Bearer of a valid capability | Holders of the epoch secret or HPKE recipient | No |

Authorization (who the knot serves) and confidentiality (what ciphertext reveals) are separate layers. See [Visibility](./08-visibility.md).

## Control plane

Writes are local. The author signs a record, the knot inserts it into the MST, the knot signs a new commit. Followers pull the new head and the missing blocks. There is no inbox POST to every follower. There is no required global firehose.

Live updates are an optional WebSocket subscribe on the author’s knot, capability-gated when the collection is not public.

## Trust boundaries

| Component | In the TCB? | If it is malicious |
| --- | --- | --- |
| Identity key / wallet | Yes | Attacker is the identity until rotation |
| Device key | Scoped | Limited to the capability’s command and TTL |
| Knot | Availability and `vis: cap` plaintext | Cannot forge public or sealed records; can withhold bytes; can read `vis: cap` |
| Replica / other knots | Availability | Same as knot for copies they hold |
| Loom | No | Can omit or rank; cannot forge |
| Gateway | No for AP; verification-only for Nostr/AT | Can lie about AP; cannot forge Nostr/AT if clients verify |
| DNS / SNS | Handle binding | Can seize the name, not the DID |
| `did:sol` on-chain document | Optional transparency | Can be withheld; generative DID still works |

## Self-hosting

A conforming `knot.public` MUST run on a single machine with:

- An HTTP server
- Storage for MST nodes, records, commits, and blobs
- The identity’s or a delegated device’s signing capability for commits it produces
- Outbound HTTP to resolve handles and to pull other repos the operator follows

It MUST NOT require a relay, a loom, a PLC directory, or an AppView to accept writes or to serve the operator’s own data to clients that know the knot URL.

## Application layer

Core collections (`sys/*`, graph, labels, circles) are specified here. Applications add collections. Unknown collections sync. Example applications (post, article, annotation, media, album, page, circle chat) are specified so the substrate is demonstrable; they are not the protocol. Worked examples: [Use cases](./20-use-cases.md).

## Why pull

ActivityPub delivers by pushing into inboxes. Cost grows with audience. Failure is silent. Tangled followers remember the author’s knot location (from `sys/id` and handle resolution) and pull the MST. Cost grows with the bytes you read. Failure is visible: the head does not move.

Nostr relays store events and let clients subscribe. There is no authoritative repository, so clients poll many relays and hope. Tangled has an authoritative signed head per identity, and any number of replicas.

AT Protocol pulls too, then inserts a relay that concatenates every public commit into a firehose so AppViews can exist. Tangled looms MAY do that for public data. They MUST NOT be required for two knots to sync.
