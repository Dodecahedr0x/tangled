---
title: Terminology
slug: terminology
order: 1
---

# Terminology

This page defines every acronym used in the Tangled specification and the protocol nouns that those documents rely on. Prose MUST expand an acronym on first use in each document and MUST have a row here.

Protocol nouns (knot, repo, circle) are in [Glossary](#glossary). Acronyms are in [Acronyms](#acronyms).

## Acronyms

| Acronym | Expansion | Meaning in Tangled | See |
| --- | --- | --- | --- |
| AEAD | Authenticated Encryption with Associated Data | Symmetric encryption of record bodies under an epoch secret or Hybrid Public Key Encryption (HPKE) context. | [Cryptography](./05-cryptography.md) |
| ACL | Access Control List | Informal name for the capability system. Tangled does not store a central who-can-do-what table; it verifies capability chains. | [Capabilities](./09-capabilities.md) |
| AP | ActivityPub | W3C federated social protocol. Tangled gateways MAY speak AP. AP objects are not author-signed; imports are gateway-attested. | [Interoperability](./16-interoperability.md) |
| API | Application Programming Interface | The Hypertext Transfer Protocol (HTTP) methods a knot exposes. | [HTTP API](./12-http-api.md) |
| ASCII | American Standard Code for Information Interchange | Restricted character set used in the Solana off-chain signing envelope when the digest is hex-encoded. | [Cryptography](./05-cryptography.md) |
| ATProto | Authenticated Transfer Protocol | Bluesky’s protocol. Compared in this spec; a gateway MAY speak a subset of its XRPC methods. | [Comparison](./19-comparison.md) |
| BLAKE3 | BLAKE3 hash function | The only content hash. 256-bit digest. | [Cryptography](./05-cryptography.md) |
| C2S | Client-to-Server | ActivityPub client API. A gateway MAY expose C2S as a facade over the local repo. | [Interoperability](./16-interoperability.md) |
| CAR | Content Addressable aRchive | IPLD file format used to export and sync repositories. | [Synchronization](./11-sync.md) |
| CBOR | Concise Binary Object Representation | Binary encoding. Tangled uses DAG-CBOR, the canonical IPLD profile of CBOR. | [Data model](./06-data-model.md) |
| CDDL | Concise Data Definition Language | Schema language for CBOR types in `spec/cddl/`. | [Data model](./06-data-model.md) |
| CID | Content Identifier | CIDv1 naming a DAG-CBOR block hashed with BLAKE3-256. | [Data model](./06-data-model.md) |
| CRDT | Conflict-free Replicated Data Type | Version 1 uses a specified CRDT for `pub.tangled.page` (Replicated Growable Array (RGA) text, Last-Writer-Wins (LWW) register title and blob, plus signed compaction). Other core collections use Last-ts-wins among causal heads. | [Data model](./06-data-model.md) |
| DAG | Directed Acyclic Graph | IPLD linking: blocks point at other blocks by CID. Repositories are DAGs. | [Repositories](./07-repositories.md) |
| DAG-CBOR | DAG profile of CBOR | Canonical CBOR used on the wire and as the input to BLAKE3. | [Cryptography](./05-cryptography.md) |
| DHKEM | Diffie–Hellman Key Encapsulation Mechanism | HPKE KEM. Tangled uses DHKEM(X25519, HKDF-SHA256). | [Cryptography](./05-cryptography.md) |
| DID | Decentralized Identifier | `did:sol:<base58-pubkey>`. Generative by default. | [Identifiers](./04-identifiers.md) |
| DNS | Domain Name System | One handle type: TXT at `_tangled.<domain>` or HTTPS well-known. | [Resolution](./13-resolution.md) |
| DoS | Denial of Service | Threat class, including forced-upgrade by spam-adding circle members. | [Threat model](./18-threat-model.md) |
| E2EE | End-to-End Encryption | Sealed records: the knot stores ciphertext. Distinct from capability gating. | [Visibility](./08-visibility.md) |
| Ed25519 | Edwards-curve Digital Signature Algorithm, 25519 | Solana’s signature scheme. Identity keys and record signatures. | [Cryptography](./05-cryptography.md) |
| FID | Farcaster ID | Numeric on-chain identity in Farcaster. Bindable via `sys/binding`. | [Interoperability](./16-interoperability.md) |
| HKDF | HMAC-based Key Derivation Function | Used inside HPKE and to derive application keys from an epoch secret. | [Cryptography](./05-cryptography.md) |
| HMAC | Hash-based Message Authentication Code | Building block of HKDF. | [Cryptography](./05-cryptography.md) |
| HPKE | Hybrid Public Key Encryption | RFC 9180. 1:1 sealed records and small-group epoch-secret wraps. | [Cryptography](./05-cryptography.md) |
| HTTP | Hypertext Transfer Protocol | Transport for the knot API. HTTP/1.1 and HTTP/2. | [HTTP API](./12-http-api.md) |
| HTTPS | HTTP Secure | HTTP over Transport Layer Security (TLS). Required for production knots. | [HTTP API](./12-http-api.md) |
| IPLD | InterPlanetary Linked Data | Content-addressed data model. CID, DAG-CBOR, CAR. | [Data model](./06-data-model.md) |
| IRI | Internationalized Resource Identifier | RFC 3987. ActivityPub object `id` values are IRIs; the canonical `foreign_id` for `activitypub` is that `id`. | [Interoperability](./16-interoperability.md) |
| JSON | JavaScript Object Notation | Debug and HTTP mapping of DAG-CBOR. Not canonical for hashing. | [Data model](./06-data-model.md) |
| JWT | JSON Web Token | Not used. Capabilities are DAG-CBOR UCAN-profile certificates, not JWTs. | [Capabilities](./09-capabilities.md) |
| LWW | Last-Writer-Wins | Per-path register in the page CRDT (`title`, `blob`): later record `ts` wins; CID tie-break. Distinct from Last-ts-wins, which picks a whole record. | [Data model](./06-data-model.md) |
| KEM | Key Encapsulation Mechanism | HPKE primitive. See DHKEM. | [Cryptography](./05-cryptography.md) |
| MIME | Multipurpose Internet Mail Extensions | Media type string stored with a blob (`image/png`). `getBlob` `Content-Type` stays `application/octet-stream`; the stored type is `X-Tangled-Media-Type`. | [HTTP API](./12-http-api.md) |
| MLS | Messaging Layer Security | RFC 9420. Large-group distributor of the circle epoch secret. Version 1 required for group-capable implementations. | [Circles](./10-circles.md) |
| MST | Merkle Search Tree | Ordered Merkle tree keyed by `collection/rkey`. Repository data structure. | [Repositories](./07-repositories.md) |
| NFC | Normalization Form Canonical Composition | Unicode normalization. Page CRDT `run` values MUST NOT be NFC-normalized before splitting into scalars. | [Data model](./06-data-model.md) |
| NIP | Nostr Implementation Possibility | Nostr extension documents. Cited where Tangled maps kinds. | [Interoperability](./16-interoperability.md) |
| OpenAPI | OpenAPI Specification | Machine-readable HTTP API in `spec/http/openapi.yaml`. | [HTTP API](./12-http-api.md) |
| OR-Set | Observed-Remove Set | Set CRDT in which a remove observes specific adds. Not used as a collection merge class in v1 (album `items` stay Last-ts-wins). | [Data model](./06-data-model.md) |
| PDA | Program Derived Address | Solana account address derived from seeds; used for the SNS `.sol` name registry. | [Resolution](./13-resolution.md) |
| PDS | Personal Data Server | AT Protocol name for a user host. A Tangled knot is the analogous unit, not a PDS. | [Comparison](./19-comparison.md) |
| PLC | Public Ledger of Credentials | AT Protocol DID method (`did:plc`) and its directory. Tangled does not use PLC. | [Identifiers](./04-identifiers.md) |
| RFC | Request for Comments | IETF documents. Tangled profiles RFC 2119, 8174, 8949, 9180, 9420, 9421, 9530. | [Conformance](./02-conformance.md) |
| RGA | Replicated Growable Array | List CRDT used for `pub.tangled.page` text. Insert after an atom id; concurrent siblings order by atom-id descending. A compact snapshot keeps original atom ids so concurrent ops still parent. | [Data model](./06-data-model.md) |
| RPC | Remote Procedure Call | Knot methods are HTTP RPCs in the `tangled.*` namespace. | [HTTP API](./12-http-api.md) |
| S2S | Server-to-Server | ActivityPub federation. A gateway MAY speak S2S. | [Interoperability](./16-interoperability.md) |
| SHA-256 | Secure Hash Algorithm 256-bit | Used inside HKDF-SHA256 and MLS. Not the content hash (that is BLAKE3). | [Cryptography](./05-cryptography.md) |
| SIWS | Sign In With Solana | Human session login to a knot (`tangled.session.challenge` / `tangled.session.verify`, cookie `tangled_session`). Not used to sign records. | [HTTP API](./12-http-api.md#sessions-siws) |
| SNS | Solana Name Service | On-chain `.sol` name → DID mapping via the Bonfida name program and Solana JSON-RPC. An optional handle type. | [Resolution](./13-resolution.md) |
| SQLite | SQLite database | Recommended local store for a self-hosted knot. Not a wire requirement. | [Architecture](./03-architecture.md) |
| TCB | Trusted Computing Base | Identity, signatures, and encryption are in the TCB. Looms and gateways are not. | [Architecture](./03-architecture.md) |
| TLD | Top-Level Domain | SNS `.sol` parent account used as a PDA seed. | [Resolution](./13-resolution.md) |
| TLS | Transport Layer Security | Required for production HTTPS. | [HTTP API](./12-http-api.md) |
| TTL | Time To Live | Capability expiry and dual-stack max-skew. Offline capability checks rely on TTL. | [Capabilities](./09-capabilities.md) |
| UCAN | User Controlled Authorization Network | Capability certificate family. Tangled profiles UCAN; it does not use JWT-UCAN as the wire format. | [Capabilities](./09-capabilities.md) |
| URI | Uniform Resource Identifier | Handles, DIDs, collection names, HTTP endpoints. | [Identifiers](./04-identifiers.md) |
| URL | Uniform Resource Locator | A URI that locates a knot or well-known resource. | [Resolution](./13-resolution.md) |
| UTF-8 | Unicode Transformation Format, 8-bit | Text encoding for JSON mappings and UTF-8 off-chain messages. | [Cryptography](./05-cryptography.md) |
| UUID | Universally Unique Identifier | Not required. Record keys (`rkey`) are TID-like or caller-chosen. | [Data model](./06-data-model.md) |
| W3C | World Wide Web Consortium | Publisher of DID Core, ActivityPub, and HTTP Message Signatures. | [Identifiers](./04-identifiers.md) |
| Content-Digest | RFC 9530 content digest | HTTP header naming a hash of the request body (`sha-256=:…:`). Required covered component when HTTP Message Signatures are present on an ActivityPub import. | [Interoperability](./16-interoperability.md) |
| X25519 | X25519 Diffie–Hellman | Key-agreement curve. Listed separately in `sys/id`; not derived from the Ed25519 identity key for ECDH. | [Identifiers](./04-identifiers.md) |
| XRPC | AT Protocol HTTP RPC style | Method names like `com.atproto.repo.getRecord`. Tangled uses `tangled.*`. Gateways MAY speak a subset of XRPC. | [Interoperability](./16-interoperability.md) |
| YAML | YAML Ain’t Markup Language | Translation tables in `spec/interop/`. | [Interoperability](./16-interoperability.md) |
| BIP | Bitcoin Improvement Proposal | BIP-340 is the Schnorr signature used on Nostr bindings. | [Interoperability](./16-interoperability.md) |
| CORS | Cross-Origin Resource Sharing | Public knot reads MUST be CORS-open (`*`). Requests with Cookie or Authorization MUST echo Origin and MUST send `Access-Control-Allow-Credentials: true`. | [HTTP API](./12-http-api.md) |
| GC | Garbage Collection | Knots MAY GC blob bytes after a delete; tombstones remain. Page compact-GC MAY omit pre-compact op blocks once every live head descends from the compact. | [Data model](./06-data-model.md) |
| QR | Quick Response code | Out-of-band way to learn a knot URL. | [Resolution](./13-resolution.md) |
| UI | User Interface | Clients MUST NOT expose HPKE vs MLS in UI. | [Circles](./10-circles.md) |
| WebCrypto | Web Cryptography API | Browsers MAY keep device keys non-extractable. | [Migration](./17-migration.md) |

## Glossary

**Annotation pattern.** A member writes a sealed record in their own repository, encrypted to a circle, with envelope `circle` set and optional `ref` pointing at a target CID. Default write pattern for social applications. Fetch uses a membership grant (`circle/read`).

**Attested.** A claim whose authenticity rests on a gateway or host, not on the original author’s signature. ActivityPub imports are attested.

**Backlink index.** A knot-local or loom index of public records keyed by envelope `ref` (or body `reply`). Exposed as `tangled.sync.queryBacklinks` and `tangled.loom.query?type=ref`. Not a global inbox.

**Blob.** Opaque bytes addressed by CID, stored beside the repository. Media, CAR segments, ciphertext payloads larger than a record.

**Canonical foreign_id.** The protocol-defined identifier of a foreign object (Nostr event id, AT strong URI, ActivityPub `id` IRI, Farcaster message hash). The `sys.foreign` rkey is derived from `protocol` plus this value.

**Capability (cap).** A delegable, attenuable, expiring certificate that authorizes a command on a resource. Presented to a knot to fetch `vis: cap` or `vis: sealed` bytes.

**Circle.** A group with membership and a read/write policy. User operations are create, add member, remove member, send, leave. The cryptographic distributor (HPKE wrap vs MLS) is not a user-facing mode.

**Circle-scoped record.** A record whose envelope `circle` field is set. `vis` MUST be `sealed`. Fetch MAY be authorized by `circle/read` on that circle.

**Collection.** A named prefix in a repository, written as a reverse-DNS type such as `pub.tangled.post` or `sys/id`.

**Compact.** A signed `pub.tangled.page` record whose body is a snapshot of the live Replicated Growable Array (RGA) and Last-Writer-Wins (LWW) registers plus a frontier of included op CIDs. Not a tombstone. Last-ts-wins collections do not compact in version 1.

**Commit.** A signed snapshot of a repository: DID, MST root CID, revision, previous commit CID, Solana signature.

**Device key.** A Solana keypair that is not the identity key, authorized by a capability to sign on behalf of the identity for a limited command set.

**Distributor.** The mechanism that delivers `epoch_secret[N]` to circle members: HPKE wrap (small-group) or MLS exporter (large-group). Chosen automatically.

**Dual-stack epoch.** The single transition epoch after HPKE→MLS grow during which the new epoch secret is available both as an MLS exporter secret and as HPKE wraps, so offline members keep reading.

**Epoch secret.** A symmetric key that Authenticated Encryption with Associated Data (AEAD) uses for circle application payloads at epoch `N`.

**Fiber.** Informal name for a capability. Normative text says **capability**.

**Foreign binding.** A `sys/binding` record that proves a Tangled identity controls (or is attested as) an identity on another network.

**Frontier.** On a page compact, the set of same-key record CIDs that are the causal heads of the folded prefix. Envelope `prev` MUST be that set. Ops not in the prev-closure of the frontier are concurrent with the compact.

**Gateway.** A knot that also speaks a foreign protocol and stores translations as `sys/foreign` envelopes.

**Generative DID.** A `did:sol` that has no on-chain document. The identifier is the public key. Default. The **generative document** has only the Ed25519 verification method. The **operational document** adds `keyAgreement` from `sys/id`.

**HTTP Message Signatures.** RFC 9421 signatures over HTTP components (`Signature-Input`, `Signature`). Tangled verifies the `ed25519` subset with Content-Digest on ActivityPub import. Covered set: `@method` `@path` `content-digest` (required); `@authority` / `host` and `content-type` MUST verify if listed.

**Included set (`I(C)`).** On a page compact `C`, the CID set of folded ops. Listed on the compact as `included`. The authoring knot MUST retain those blocks.

**Handle.** A human-readable name bidirectionally bound to a DID: a DNS name or an SNS name.

**Head.** The latest commit CID of a repository, as advertised by a knot.

**History share.** Circle policy that, on add-member, also wraps prior epoch secrets to the new member. Off by default. Not a side effect of size migration.

**Identity key.** The Solana Ed25519 keypair that *is* the account.

**Invocation.** A request signed by a capability’s audience, carrying the proof chain.

**Knot.** The host process. Serves repositories and blobs. Self-hostable as one process and a local database.

**Label.** A signed record asserting a classification of another record or DID. Clients compose labelers. Knots are not the moderation authority.

**Last-ts-wins.** Merge class for most collections: among same-key causal heads, later `ts` is the live record; CID tie-break. A merge record lists concurrent heads in `prev`. Distinct from the page CRDT, which applies every uncovered op in the apply set (and compact snapshots). Last-ts-wins collections do not compact in version 1.

**Loom.** Optional indexer of public heads. Not in the Trusted Computing Base (TCB).

**Membership grant.** A `circle/read` capability the circle owner issues to a member DID. Revoked on remove. Independent of knot URL.

**Op-id.** Identifier of a CRDT operation. For `pub.tangled.page`, the op-id is the signed record’s CID. Atoms inside an insert are `(op-id, i)`. A compact does not mint new atom ids.

**Record.** A typed, content-addressed object signed by a Solana key, stored in a repository.

**Repo (repository).** One identity’s signed MST plus its blobs.

**rkey.** The record key inside a collection. Unique per collection per repo.

**Sealed.** Visibility class: the body is ciphertext. The knot stores opaque bytes.

**Shared-repo pattern.** A dedicated circle DID owns an MST; members hold `repo/write` capabilities and sign as device keys (`author` is the circle DID). The hosting knot signs commits. Used for wikis and datasets. Any such writer MAY compact a page.

**Snapshot.** In page compaction, the RGA atom table (original atom ids, parents, scalars, delete flags) and LWW register candidates (`ts`, CID, value) obtained by applying the included set. Distinct from a repository commit snapshot.

**Strand.** Informal name for a repository. Normative text says **repo**.

**Threshold.** Spec constant `CIRCLE_MLS_THRESHOLD = 16`. The member count at which a circle that has never used MLS MUST migrate to MLS. Not a client setting.

**TID.** Timestamp identifier. 13-character Crockford base32 of `(unix_micros << 10) | entropy10`. Recommended for record keys; REQUIRED for commit `rev`.

**Unsigned record.** The DAG-CBOR map of a record with the `sig` field omitted. Input to BLAKE3 for the signing envelope.

**Visibility (`vis`).** Record field: `public`, `cap`, or `sealed`.

**Welcome.** MLS message that introduces a member to a group. Published as a sealed record in the circle log. RFC 9420 seals Welcome to KeyPackage `init_key`, which MUST differ from the leaf HPKE key (the Tangled X25519 `agreement` key).

**Yarn.** Informal name for a schema document. Normative text says **schema**.
