---
title: Interoperability
slug: interoperability
order: 16
---

# Interoperability

Tangled is a substrate that other networks can bind to. Bridges MUST follow this chapter so they round-trip with each other instead of each inventing a mapping.

## Foreign identity bindings

Collection `sys.binding`, rkey = TID.

```
Binding = {
  v: 1,
  tangled: did,             ; did:sol:…
  protocol: "nostr" / "atproto" / "activitypub" / "farcaster",
  foreign: text,            ; npub/hex, AT DID, actor URI, or FID string
  nonce: bstr .size 16,
  ts: uint,
  attested: bool,           ; true only for activitypub
  foreignSig: bstr,         ; protocol-specific
  ? foreignProof: any
}
```

The Tangled record signature proves the Solana key. `foreignSig` proves the foreign key (or host). Both MUST verify. `sys/id.bindings` SHOULD list the CID.

### Nostr

`foreign` is the 64-hex public key. `foreignSig` is a Schnorr signature (BIP-340) over SHA-256 of the UTF-8 string:

```
tangled-binding:v1:did:sol:<base58>:<hex(nonce)>
```

Alternatively, `foreignProof` MAY be a NIP-01 event of kind `30078` (arbitrary app data) with `content` equal to the Tangled DID and `tags` including `["d","tangled-binding"]`. Then `foreignSig` is that event’s `sig`. Verifiers MUST check `pubkey` equals `foreign`.

### AT Protocol

`foreign` is the AT DID (`did:plc:…` or `did:web:…`). `foreignProof` MUST include the DID document. `foreignSig` is the AT signing key’s signature over the same `tangled-binding:v1:…` bytes, using the AT key type (k256 or p256 as in the DID document). Verifiers MUST check the signing key is listed in that document.

### ActivityPub

ActivityPub actors do not have a user key in the ratified specification. `attested` MUST be `true`. `foreign` is the actor URI. `foreignProof` is either:

- An HTTP Message Signature over `GET {actorURI}` that includes header `Tangled-Binding: did:sol:…`, or
- `https://{actor-host}/.well-known/tangled-binding.json` listing `{ "did": "did:sol:…", "actor": "https://…" }` served on the actor host.

Clients MUST display AP-bound identities as **host-attested**, never as key-equals-person.

### Farcaster

`foreign` is the FID decimal string. `foreignSig` is Ed25519 from a currently valid Farcaster signer key over `tangled-binding:v1:…`. `foreignProof` MUST include the signer key and a proof it is registered to the FID (hub message or on-chain lookup description).

## Envelopes

Collection `sys.foreign`, rkey derived as in [Idempotent import](#idempotent-import).

```
Foreign = {
  v: 1,
  protocol: text,           ; MUST be "nostr" / "atproto" / "activitypub" / "farcaster"
  foreign_id: text,         ; canonical id, see below
  bytes_cid: cid,           ; raw blob of original bytes
  translator: did,          ; gateway DID
  attested: bool,
  ? mapped: cid             ; CID of the Tangled application record, if any
  ? mapped_lossy: bool
}
```

**Round-trip:** exporting a previously imported object MUST emit the original bytes from `bytes_cid` when present. Gateways MUST NOT re-encode Nostr events or AT records if `bytes_cid` exists.

## Canonical foreign_id

`foreign_id` MUST be the **canonical** identifier of the foreign object, not a gateway-local alias. Two gateways importing the same object MUST produce the same `(protocol, foreign_id)` pair.

| `protocol` | Canonical `foreign_id` |
| --- | --- |
| `nostr` | Lowercase 64-hex NIP-01 event id (SHA-256 of the serialized event) |
| `atproto` | Strong AT URI: `at://<did>/<collection>/<rkey>#<record-cid>` with the DID, collection, rkey, and record Content Identifier (CID) as in the AT commit. The `#<record-cid>` fragment is REQUIRED so an edit is a new object |
| `activitypub` | The object’s `id` Internationalized Resource Identifier (IRI), exactly as published, as an absolute IRI. Gateways MUST NOT invent a fragment or trailing slash. Scheme `http` / `https` MUST be lowercase; the rest of the IRI MUST be copied verbatim |
| `farcaster` | Lowercase hex of the Farcaster message hash |

Unknown `protocol` values MAY be imported; then `foreign_id` MUST be a stable, documented identifier for that protocol. Gateways that speak the four listed protocols MUST use this table.

## Idempotent import

The rkey of `sys.foreign` MUST be the lowercase hex of BLAKE3-256(`UTF-8(protocol) || 0x00 || UTF-8(foreign_id)`) where `foreign_id` is the canonical value. A second import of the same `(protocol, foreign_id)` MUST update that record in place (`prev` pointing at the prior envelope CID) and MUST NOT insert a second envelope. If the computed rkey already holds an envelope whose stored `foreign_id` differs, that is a hash collision: the knot MUST NOT overwrite; it MUST reject the write.

If `mapped` is set and that CID is still in the local MST as a native application record, a later import MUST NOT write a second application record.

**Native export.** Exporting a Tangled-authored object MUST write `sys.foreign` with `mapped` equal to the native record CID and `bytes_cid` equal to the bytes that were actually sent. `foreign_id` MUST be the canonical id of those bytes (for example the Nostr event id after the event is signed). A later export of the same native CID MUST reuse that envelope and MUST emit `bytes_cid`; it MUST NOT mint a new foreign id. If the native record has a new CID (an edit), the gateway MAY create a new envelope with a new canonical `foreign_id`.

`mapped_lossy: true` MUST be set on the envelope when the gateway also writes a Tangled application record and the mapping truncates or stubs.

## Duplicate envelopes (pre-canonical or divergent gateways)

If two envelopes for the same object were written with different `foreign_id`s (gateways that ignored this chapter, or a protocol that had no canonical id yet), clients MUST detect duplicates as follows, in order:

1. **`bytes_cid` equality.** Distinct `sys.foreign` records whose `bytes_cid` values are equal are the same object. Clients MUST render one and MAY keep both envelopes.
2. **Id inside the bytes.** If `bytes_cid` differs (re-encoding) but the foreign bytes contain a protocol id, clients MUST extract it with the table above (Nostr event id from the event, AT record CID from the commit proof, ActivityPub `id` IRI from the JSON-LD, Farcaster message hash) and compare canonical `(protocol, foreign_id)`.
3. **`mapped` equality.** Distinct envelopes with the same `mapped` CID are the same native record exported twice.

Clients MUST NOT treat distinct canonical `foreign_id`s as duplicates without one of these equalities. Merging duplicate envelopes (one surviving rkey, the other tombstoned with `prev`) is OPTIONAL operator hygiene.

## Translation tables

Normative tables live in `spec/interop/*.yaml`. Summary:

| Tangled | ActivityPub | Nostr | AT Protocol |
| --- | --- | --- | --- |
| `pub.tangled.post` | `Note` | kind `1` | `app.bsky.feed.post` |
| `pub.tangled.article` | `Article` | kind `30023` (NIP-23) | `app.bsky.feed.post` with long text, or a custom lexicon — **lossy** |
| `pub.tangled.annotation` | `Note` (`inReplyTo`) | kind `1` (`e` tag) | `app.bsky.feed.post` (`reply`) — **lossy** |
| `tangled.graph.follow` | `Follow` | kind `3` contact list (NIP-02) / NIP-51 | `app.bsky.graph.follow` |
| `tangled.graph.block` | `Block` | NIP-51 mute/block | `app.bsky.graph.block` |
| `vis: sealed` | stub `Note` content `"(encrypted)"` or omit | MUST NOT export plaintext | MUST NOT export plaintext |

Lossy mappings MUST set `mapped_lossy: true` on the envelope when a gateway also writes a Tangled application record.

## Gateway knots

A knot MAY speak foreign protocols. It advertises them in `tangled.node.describe` → `protocols`.

### ActivityPub

C2S/S2S facade over the local repo. Outbox items are public Tangled posts mapped to `Note`. Inbox items become `sys.foreign` plus, if mappable, a `pub.tangled.post` with `attested: true`. The gateway DID is `translator`. Dual-publish (a native public post also emitted as an outbox `Note`) MUST still write the envelope; if that `Note` returns on S2S, idempotent rkey collision MUST update the envelope only.

### Nostr

A NIP-01 relay interface. `EVENT` of kind 1 with a valid sig becomes an envelope + mapped post, `attested: false`. `REQ` filters are served from public collections. Sealed records MUST NOT appear.

### AT Protocol

Subset: `com.atproto.repo.getRecord`, `com.atproto.repo.listRecords`, `com.atproto.sync.getRepo` over public collections whose schemas have an AT lexicon mapping. The MST is the same MST. This is a facade, not an AT PDS.

## Honesty

| Foreign protocol | Import authenticity |
| --- | --- |
| Nostr | Independently verifiable (event id + sig) |
| AT Protocol | Independently verifiable (record + commit proof) if the gateway stored the proof |
| ActivityPub | Gateway-attested |
| Farcaster | Independently verifiable if signer proof is stored |

Clients MUST surface `attested` in UI when rendering imported AP objects.
