---
title: Resolution
slug: resolution
order: 13
---

# Resolution

Resolution turns a handle or a DID into a knot URL and a DID document. It does not confer trust beyond what signatures later verify.

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

Let `n` be an SNS domain.

1. Resolve the SNS record per the Solana Name Service program for the cluster implied by the DID (mainnet default).
2. The record MUST contain a field `tangled` or, if absent, the account owner MUST equal the identity public key.
3. Fetch `sys/id`, verify `n` is in `alsoKnownAs`.

SNS seizure: whoever controls the SNS domain controls the handle, not the DID. Same as DNS.

## Bidirectional rule

Clients MUST reject handle claims that fail either direction. A DID listing a handle it does not control is a lie. A handle pointing at a DID that does not list it is a lie.

## Head hints

DNS TXT `head=<cid>` and SNS `head` reduce stale-head windows. Clients SHOULD fetch the hinted commit and verify it is a descendant of (or equal to) their cache, or a valid rotation/recovery.

## Negative caching

Failed DNS lookups SHOULD be cached per DNS TTL. Failed well-known lookups SHOULD be cached 5 minutes. Clients MUST NOT cache a successful DID→knot mapping longer than 24 hours without re-checking `sys/id`.
