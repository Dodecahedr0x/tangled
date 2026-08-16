---
title: Identifiers
slug: identifiers
order: 4
---

# Identifiers

A Tangled identity is a Solana account: a 32-byte Ed25519 public key. Display form is Bitcoin-alphabet base58, 43 or 44 characters. Every wallet is already an identity. There is no registration. There is no on-chain rent required to exist.

## DID

The Decentralized Identifier (DID) is generative [`did:sol`](https://identity-com.github.io/sol-did/did-method-spec.html):

```
did:sol:<base58-pubkey>
```

Implementations MUST accept `did:sol:` on mainnet with no cluster prefix. Implementations MUST accept `did:sol:devnet:` and `did:sol:testnet:` when talking to those clusters. The identifier without a cluster prefix denotes mainnet-beta.

Tangled MUST NOT invent a `did:tangled` method.

A generative DID has no on-chain document. Resolution of a generative DID **without** a verified `sys/id` MUST produce a DID document whose sole verification method is the public key itself, with `capabilityInvocation` and `authentication` granted to that key. That **generative document** MUST NOT include `keyAgreement`. Implementations MUST NOT invent an X25519 key by converting the identity Ed25519 key.

```json
{
  "id": "did:sol:<base58>",
  "verificationMethod": [{
    "id": "did:sol:<base58>#key",
    "type": "Ed25519VerificationKey2020",
    "controller": "did:sol:<base58>",
    "publicKeyBase58": "<base58>"
  }],
  "authentication": ["did:sol:<base58>#key"],
  "capabilityInvocation": ["did:sol:<base58>#key"]
}
```

Persisting a DID document on-chain via the `did:sol` program is OPTIONAL. When present, it is a transparency log for rotation, not a gate on existence. A knot MUST serve `sys/id` even if no on-chain document exists.

When a knot or client has a verified `sys/id` for the DID (or equivalent hosted identity), it MUST return the **operational document**: the generative methods plus `keyAgreement` for the X25519 `agreement` public key from `sys/id`. The agreement verification method id is `did:sol:<base58>#agreement`. Type is `X25519KeyAgreementKey2020`. The public key is `publicKeyBase58` (Bitcoin-alphabet base58 of the 32 raw bytes), matching the Ed25519 method encoding.

`tangled.identity.resolve` MUST return that operational document in `document` when `sys/id` is available, and the generative document otherwise. Clients that only have the 32-byte Ed25519 key MUST emit the generative document.

## sys/id

The repository MUST contain a record at collection `sys.id` with rkey `self` (written `sys/id` in prose). That record is the operational DID document for Tangled.

CDDL: `IdRecord` in `spec/cddl/id.cddl`.

Fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `v` | `1` | Version |
| `pubkey` | `bstr .size 32` | Identity Ed25519 public key, raw |
| `agreement` | `bstr .size 32` | X25519 public key for Hybrid Public Key Encryption (HPKE) and Messaging Layer Security (MLS). MUST be a distinct key. Implementations MUST NOT convert the identity Ed25519 key to X25519 for Diffie–Hellman. |
| `rotation` | `[+ bstr .size 32]` | Ordered Solana public keys that may rotate the identity. Index 0 is highest priority. |
| `recovery` | `[* bstr .size 32]` | Keys that may start a recovery. Empty is allowed; losing the identity key is then permanent. |
| `endpoints` | `[+ Endpoint]` | Knot URLs. `type` is `"knot"`. First is preferred. |
| `alsoKnownAs` | `[* text]` | Handles this DID claims: DNS names and SNS names. |
| `bindings` | `[* cid]` | CIDs of `sys/binding` records. |
| `headHint` | `cid` (optional) | Last commit CID this identity published, for stale-head detection. |

The record is signed like any other record, by the current identity key or by a rotation key during a rotation commit.

Clients MUST cache the latest `sys/id` CID they have verified. A knot that serves an older `sys/id` is stale or hostile. Clients SHOULD compare `headHint` and the served commit head.

## Handles

A handle is a human-readable name bound to a DID. Two types:

1. **DNS.** A hostname such as `alice.example.com`. Proof is either:
   - A DNS TXT record at `_tangled.<hostname>` whose value is the DID, or
   - `https://<hostname>/.well-known/tangled-did` serving `text/plain` with the DID as the sole line, or JSON `{"did":"did:sol:…"}`.
2. **SNS.** A Solana Name Service (SNS) `.sol` domain. Proof is the name-registry account on the Bonfida name program: a `tangled` field naming the DID, or, if that field is absent, the 32-byte account owner encoded as `did:sol:`. Resolution procedure (Program Derived Address (PDA), 96-byte header, JSON-RPC) is in [Resolution](./13-resolution.md).

Bidirectional binding: `sys/id.alsoKnownAs` MUST list the handle, and the handle proof MUST name the DID. Clients MUST reject one-sided claims.

A DID MAY have zero handles. A DID MAY have several. The DID is the stable identifier; the handle is a pointer.

## Rotation

Three procedures. Implementations MUST NOT conflate them.

### Device-key-only

Revoke the old device capability; issue a new `repo/write` (and related) capability to the new device public key. `sys/id.pubkey` and `sys/id.agreement` MUST NOT change. Circle epoch secrets MUST NOT be re-wrapped. See [Circles](./10-circles.md#device-key-only-rotation).

### Agreement-only

The X25519 `agreement` key is compromised or rotated on a schedule; the identity Ed25519 key is unchanged. Write a new `sys/id` with the new `agreement`. Sign the commit with the current identity key (no rotation commit). Then follow [Circles](./10-circles.md#agreement-rotation): the rotating member’s client re-wraps epoch secrets and, when the distributor is MLS, publishes an MLS Update.

### Identity key (and agreement)

To rotate the identity key:

1. Produce a new Ed25519 keypair and a new X25519 agreement key. Writers MUST rotate `agreement` whenever they rotate `pubkey`.
2. Write a new `sys/id` listing the new `pubkey`, the new `agreement`, and an updated `rotation` list that still includes a key the operator holds.
3. Sign the new repository commit with **both** the current identity key and a key from `rotation` (two signatures on the commit; see [Repositories](./07-repositories.md#rotation-commits)).
4. Optionally submit the new verification methods to the on-chain `did:sol` program.
5. Re-wrap circle epoch secrets and publish MLS Updates as specified in [Circles](./10-circles.md#agreement-rotation). The rotating member’s client MUST do this without a user-facing cryptographic mode.

The Decentralized Identifier (DID) string remains `did:sol:<original-base58>`. `sys/id.pubkey` is the current signing key. After rotation, records signed by the old key remain valid in history. New records MUST be signed by the new key. Capabilities issued by the old key remain valid until they expire or are revoked; issuers SHOULD revoke and reissue.

If the identity key is stolen and a rotation key is held:

- The operator rotates as above.
- Clients that cached the previous `sys/id` accept the rotation because it is signed by a listed rotation key.
- Clients that only know the stolen key MUST be told the new head out of band (DNS `head` TXT, SNS field, or a replica). This is the cost of having no global PLC directory.

If both identity and rotation keys are lost, the DID is unrecoverable unless a `recovery` key exists. A recovery key MAY produce a rotation commit. Knots SHOULD delay serving a recovery-only rotation for 72 hours while serving both heads, matching the recovery window idea in AT Protocol without a PLC operator.

Moving a knot (`endpoints` only) is [Migration](./17-migration.md). It MUST NOT rotate `pubkey` or `agreement`.

## Device keys

The identity key SHOULD NOT live on every laptop. A device generates its own Solana keypair. The identity issues a capability (`repo/write` attenuated to collections and a TTL) to that device public key. The device signs records with the device key. The record’s `author` remains the identity DID. The record includes `signedBy` (the device public key) and the capability CID in `proof`.

Verifiers MUST:

1. Verify the record signature against `signedBy`.
2. Verify the capability chain from the identity to `signedBy`.
3. Verify the capability authorizes the collection and has not expired or been revoked.

## Foreign identifiers

Other-network identifiers are not Tangled identities. They bind through `sys/binding` as specified in [Interoperability](./16-interoperability.md). `alsoKnownAs` MUST NOT be used as the only proof of a foreign identity.

## Comparison notes

- Nostr: the public key is the identity, but it cannot rotate. Tangled rotates.
- AT Protocol: DID plus PLC directory. Tangled uses `did:sol` generative, no directory required.
- ActivityPub: `@user@host`. The host is the identity. Tangled’s host is a location.
- Farcaster: on-chain FID plus rent. Tangled requires no rent to exist.
