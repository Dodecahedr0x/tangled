---
title: Capabilities
slug: capabilities
order: 9
---

# Capabilities

Authorization is object-capability. A capability is a certificate: issuer, audience, command, resource, attenuation, proofs, expiry, signature. Tangled profiles User Controlled Authorization Network (UCAN) semantics and encodes certificates as DAG-CBOR, not JSON Web Tokens (JWT).

## Soundness properties

Implementations MUST provide:

| Property | Meaning |
| --- | --- |
| Unforgeable | Only the issuer’s Solana key can create a valid certificate at that layer of the chain |
| Monotonic attenuation | A delegate MUST NOT amplify command, resource, or time |
| Revocable | Issuer (or an ancestor) can publish `sys.revocation`; verifiers MUST honor it when online |
| Least authority | Resource is a DID, a collection prefix, or a record CID — not “the knot” |
| Auditable | Certificates are records; they have CIDs |

## Certificate

```
Capability = {
  v: 1,
  iss: did,                 ; issuer
  aud: did,                 ; audience (may be a device did:sol)
  cmd: command,
  res: text,                ; resource URI
  nbf: uint,                ; not-before, Unix seconds
  exp: uint,                ; expiry, Unix seconds
  pol: [* caveat],          ; extra constraints
  prf: [* cid],             ; proofs (parent capabilities)
  ? meta: { * text => any },
  sig: bstr .size 64
}
```

`exp - nbf` MUST NOT exceed `CAPABILITY_MAX_TTL_SECONDS`. `exp` MUST be greater than `nbf`.

The signature uses the Solana off-chain envelope over the unsigned capability map, signed by `iss` (or a device key of `iss` with a proof that allows `cap/issue`).

## Commands

| Command | Meaning |
| --- | --- |
| `repo/read` | Fetch records and MST nodes |
| `repo/write` | Insert records (still must be signed) |
| `blob/get` | Fetch blobs |
| `blob/put` | Store blobs |
| `circle/invite` | Add members to a circle the resource identifies |
| `circle/remove` | Remove members |
| `circle/read` | Fetch circle-scoped records and referenced blobs; fetch the circle log on the owner repo |
| `knot/replicate` | Pull authorized collections as a replica |
| `cap/issue` | Issue attenuated children |
| `*` | All commands; only the identity key MAY issue `*` |

Unknown commands MUST be rejected.

## Resources

A resource URI:

```
tangled://<did>
tangled://<did>/<collection>
tangled://<did>/<collection>/<rkey>
tangled://<did>/blob/<cid>
tangled://circle/<owner-did>/<circle-rkey>
```

Attenuation: a child capability’s `res` MUST equal the parent’s `res` or be a path prefix extension of it. `tangled://did:sol:X/pub.tangled.chat` is a valid child of `tangled://did:sol:X`. The reverse is invalid.

A `tangled://circle/…` resource is **not** a prefix of `tangled://did:sol:…`. `repo/read` on an author DID does not grant `circle/read`, and `circle/read` does not grant `repo/read` on unrelated collections. `circle/read` authorizes only:

1. `sys.circle` with rkey `<circle-rkey>` on `<owner-did>`
2. `sys.circle.wrap` and `sys.circle.mls` records on `<owner-did>` whose envelope `circle` matches
3. Any record stored by the verifying knot whose envelope `circle` equals `did:sol:<owner>#<circle-rkey>`
4. Blobs referenced by records in (2) or (3)

## Circle/read verification

Verification steps 1–2 and 4–7 of [Invocation](#invocation) apply. Step 3 is replaced: the root `iss` MUST be `<owner-did>` (the author of that `sys.circle` record) or a `cap/issue` delegate recorded in that owner’s `sys/id`. The hosted DID of the knot answering the request NEED NOT equal `iss`. By writing a record with envelope `circle` set, the author delegates fetch authorization for that record to valid `circle/read` capabilities on that circle.

`res` MUST be a `tangled://circle/<owner-did>/<circle-rkey>` URI. A `circle/read` invocation whose `res` is a `tangled://did:sol:…` URI MUST be rejected. A `repo/read` capability MUST NOT authorize `circle/read`, and MUST NOT cause `getRepoAuth` to include sibling collections. Authorization vectors: `spec/vectors/circle-read-auth.json`.

## Caveats

`pol` is a list of predicates. Version 1 predicates:

| Caveat | Meaning |
| --- | --- |
| `["eq", json-pointer, value]` | Invocation args must equal |
| `["lt", json-pointer, number]` | Less than |
| `["valid_until", seconds]` | Additional cap on `exp` |
| `["max_uses", n]` | Knots that track invocations SHOULD enforce; offline verifiers MAY ignore |

Unknown caveats MUST cause rejection (fail closed).

## Invocation

A request signs an invocation:

```
Invocation = {
  v: 1,
  cap: cid,                 ; leaf capability
  prf: [+ Capability],      ; chain, leaf last, root first
  cmd: command,
  res: text,
  args: { * text => any },
  ts: uint,
  aud: did,                 ; the knot DID, or the resource DID for peer pull
  nonce: bstr .size 16,
  sig: bstr .size 64        ; audience of the leaf cap
}
```

HTTP: `Authorization: Cap <base64url(DAG-CBOR(Invocation))>`.

Verification:

1. Leaf `aud` matches the invocation signer.
2. Each certificate’s `aud` matches the next certificate’s `iss` (or the invocation signer for the leaf).
3. Root `iss` matches the resource DID’s identity (or a `cap/issue` delegate recorded in `sys/id`). For `circle/read`, the resource DID is the circle **owner**, as specified in [Circle/read verification](#circleread-verification).
4. Commands and resources attenuate monotonically.
5. `nbf <= now <= exp` for every certificate.
6. No CID in the chain is present in the issuer’s `sys.revocation` (online) OR every certificate has `exp` within the client’s accepted offline TTL (offline).
7. Signatures verify.

## Revocation

Collection `sys.revocation`, rkey = base32 of the revoked capability CID (without multibase prefix). Body: `{ cid: cid, ts: uint, reason: text }`. Signed by the issuer or an ancestor issuer.

Revocation is not transitive to unrelated chains. It IS transitive to children of the revoked CID: a chain that contains a revoked CID is invalid.

Online verifiers MUST fetch `sys.revocation` for each issuer in the chain (cache TTL RECOMMENDED 60 seconds). Offline verifiers MUST require `exp` no more than 2 hours in the future (`OFFLINE_MAX_REMAINING = 7200`) unless they have a cached revocation MST.

## Root authority

The identity key may issue any command on its DID. It does not need a parent certificate. `prf` is empty. This is the only uncertified issuer.
