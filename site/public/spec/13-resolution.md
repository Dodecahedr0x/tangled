---
title: Resolution
slug: resolution
order: 13
---

# Resolution

Resolution turns a handle or a DID into a knot URL and a DID document. It does not confer trust beyond what signatures later verify.

A **generative document** is computed from the Ed25519 public key alone (`authentication` and `capabilityInvocation`; no `keyAgreement`). An **operational document** adds `keyAgreement` from `sys/id.agreement` as specified in [Identifiers](./04-identifiers.md#did). `tangled.identity.resolve` MUST return the operational document when the resolver has `sys/id` (or equivalent hosted identity) for that DID, and MUST return the generative document otherwise.

## DID → knot

1. If the client has a cached `sys/id` for the DID, use `endpoints` whose `type` is `"knot"`, in list order, skipping endpoints that failed recently.
2. Else if an on-chain `did:sol` document exists and lists a service endpoint of type `TangledKnot`, use it. On-chain data is OPTIONAL.
3. Else the client MUST be given a knot URL out of band (QR, follow record that includes `knot`, previous session).

There is no global directory. That is intentional.

## Handle → DID (DNS)

Hostname `h`.

1. Query TXT `_tangled.h`. If a TXT value starts with `did:sol:`, that is the DID. If a TXT value starts with `head=`, it is a commit CID hint.
2. If TXT is absent, `GET https://h/.well-known/tangled-did` (Follow redirects only to the same host). Body is the DID (trimmed) or JSON `{"did":"…","head":"b…"}`.
3. Fetch `sys/id` from the knot in that DID’s endpoints (or from a well-known `https://h/.well-known/tangled-id.json` serving the record).
4. Verify `h` is in `alsoKnownAs`. If not, abort.

TXT and well-known MUST agree if both exist; if they disagree, abort.

## Handle → DID (SNS)

Let `n` be an SNS domain of the form `<label>.sol` (case-insensitive; no `/` or `:`).

SNS is resolved via Solana JSON-RPC (`getAccountInfo`) against the Bonfida SPL Name Service program. A local `solana-test-validator` is a test harness used by continuous integration; it is not a protocol requirement.

1. Derive the name-registry Program Derived Address (PDA):
   - Program: `namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX`
   - Top-Level Domain (TLD) parent (`.sol` root): `58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx`
   - Hashed name: `SHA-256("SPL Name Service" || label)` where `label` is `n` without the `.sol` suffix
   - Seeds: hashed name, 32 zero bytes (class), TLD parent
2. `getAccountInfo` on that PDA with encoding `base64`. Missing account MUST fail resolution.
3. Parse the account: 96-byte header `parent (32) || owner (32) || class (32)`, then UTF-8 data. Data MAY be JSON `{"tangled":"did:sol:…","head":"b…"}` or lines `tangled=<did>`, `did:sol:…`, `head=<cid>` (semicolon- or newline-separated).
4. The DID MUST be the `tangled` field if present; otherwise `did:sol:` plus Bitcoin-alphabet base58 of the 32-byte owner. The cluster prefix on that DID MUST match the RPC cluster the client is using (mainnet-beta default; `devnet` / `testnet` when the RPC URL names those clusters).
5. Fetch `sys/id`, verify `n` is in `alsoKnownAs`.

SNS seizure: whoever controls the SNS domain controls the handle, not the DID. Same as DNS.

## Bidirectional rule

Clients MUST reject handle claims that fail either direction. A DID listing a handle it does not control is a lie. A handle pointing at a DID that does not list it is a lie.

## Head hints

DNS TXT `head=<cid>` and SNS `head` reduce stale-head windows. Clients SHOULD fetch the hinted commit and verify it is a descendant of (or equal to) their cache, or a valid rotation/recovery.

## Negative caching

Failed DNS lookups SHOULD be cached per DNS TTL. Failed well-known lookups SHOULD be cached 5 minutes. Clients MUST NOT cache a successful DID→knot mapping longer than 24 hours without re-checking `sys/id`.
