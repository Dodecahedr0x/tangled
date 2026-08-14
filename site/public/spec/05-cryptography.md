---
title: Cryptography
slug: cryptography
order: 5
---

# Cryptography

Tangled reuses existing primitives. This chapter profiles them. It does not invent a signature scheme, a KEM, or a group ratchet.

## Algorithms

| Role | Algorithm | Notes |
| --- | --- | --- |
| Identity and record signatures | Ed25519 | Solana’s scheme. 32-byte public key, 64-byte signature |
| Content hash | BLAKE3-256 | Only content hash. SHA-256 appears only inside HKDF and MLS |
| Block identifier | CIDv1, codec `dag-cbor` (0x71), multihash `blake3` (0x1e), 32-byte digest | |
| Canonical encoding | DAG-CBOR | Input to BLAKE3. JSON is a mapping, not a hash input |
| 1:1 and small-group wrap | HPKE RFC 9180 | KEM `0x0020` DHKEM(X25519, HKDF-SHA256), KDF `0x0001` HKDF-SHA256, AEAD `0x0003` ChaCha20Poly1305 |
| Circle large-group | MLS RFC 9420 | Ciphersuite `0x0001` `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` |
| Circle application AEAD | ChaCha20-Poly1305 | Key = HKDF-SHA256(`epoch_secret[N]`, info=`tangled.epoch.aead.v1`) |

Implementations MUST reject other signature schemes for Tangled records. Foreign envelopes MAY carry foreign signatures as opaque bytes.

## DAG-CBOR canonicalization

Unsigned records, commits, capabilities, and schema documents MUST hash as DAG-CBOR with:

- Major types as in RFC 8949
- Map keys as UTF-8 strings, sorted by UTF-8 byte order
- No indefinite-length items
- No tags other than CID tag `42` as specified by DAG-CBOR
- Integers in shortest form
- No duplicate map keys

JSON mapping: CIDs become `{"/":"<base32-cid>"}` (IPLD JSON). Bytes become `{"/":{"bytes":"<base64>"}}`. Implementations MUST NOT hash JSON.

## Content Identifier

```
cid = CIDv1( 0x71 dag-cbor, 0x1e blake3, 32, blake3(dag-cbor-bytes) )
```

Display: `b` multibase (base32, lowercase, no padding) unless a document says otherwise. Test vectors use this encoding.

## Solana signatures

Public keys and signatures on the wire (DAG-CBOR) are raw bytes, 32 and 64. Display is base58.

### Signing envelope

A record signature is Ed25519 over the **Solana off-chain message** whose body is the hex digest of the unsigned record. This is mandatory so a Tangled signature cannot be a valid Solana transaction, and so hardware wallets and `solana sign-offchain-message` agree.

Construct the message as:

```
header     = 0xff || "solana offchain"          ; 16 bytes
version    = 0x00                               ; 1 byte, currently the only version
format     = 0x00                               ; 1 byte, restricted ASCII
length     = uint16le(64)                       ; 2 bytes, body length
body       = lowercase_hex( BLAKE3-256( unsigned_record_dag_cbor ) )
                                                ; 64 ASCII bytes
message    = header || version || format || length || body
signature  = Ed25519.Sign(identity_or_device_sk, message)
```

`unsigned_record_dag_cbor` is the record map with `sig` omitted.

Verifiers MUST rebuild this exact byte string and call Ed25519 verify with the author’s (or `signedBy`) public key.

Wallet-adapter `signMessage` is equivalent if and only if the client passes `message` (the full envelope bytes) to the wallet. Clients MUST NOT pass only the digest or only the CBOR.

Sign In With Solana (SIWS) authenticates a human session to a knot’s HTTPS surface. SIWS MUST NOT be used as a record signature.

### Commit signatures

A commit signs the same envelope construction, with `unsigned_record_dag_cbor` replaced by the unsigned commit map (`sig` and `sigRotation` omitted). Rotation commits carry a second signature `sigRotation` over the same message, verified against a key listed in the previous `sys/id.rotation`.

## HPKE

Used for:

1. **1:1 sealed records.** The body is an HPKE seal to the recipient’s X25519 `agreement` key. Info string: `tangled.hpke.record.v1`. Associated data: the unsigned record’s DAG-CBOR without `body` and `sig`.
2. **Small-group epoch wraps.** Each member receives HPKE wrap of `epoch_secret[N]`. Info string: `tangled.hpke.epoch.v1`. Associated data: `circle_did || uint64be(N)`.

Single-shot HPKE (base mode) MUST be used. The encapsulating key (`enc`) is stored beside the ciphertext.

## MLS

Ciphersuite MUST be `0x0001`. Credential type: basic credential whose identity is the 32-byte Solana public key (not the DID string). Application ID: `tangled.circle.v1`.

The MLS **exporter secret** with label `tangled.epoch` and context `uint64be(N)` and length 32 is `epoch_secret[N]` when the distributor is MLS.

Welcome and Commit messages are stored as sealed records in the circle log (`sys.circle.mls`, rkey = TID). They are not user-visible.

Group-capable implementations MUST implement MLS. Knots that only advertise `knot.public` MAY omit it.

## Epoch application keys

```
epoch_secret[N]     : 32 bytes   (from HPKE wrap or MLS exporter)
aead_key            = HKDF-SHA256-Extract-Expand(
                        salt = "tangled.epoch.aead.v1",
                        ikm  = epoch_secret[N],
                        info = circle_did,
                        L    = 32)
nonce_base          = HKDF-SHA256-Extract-Expand(
                        salt = "tangled.epoch.nonce.v1",
                        ikm  = epoch_secret[N],
                        info = circle_did,
                        L    = 12)
```

Record AEAD nonce = `nonce_base` XOR first 12 bytes of BLAKE3(unsigned record without body). Ciphertext is ChaCha20-Poly1305 over the body bytes.

## Constants

| Name | Value |
| --- | --- |
| `CIRCLE_MLS_THRESHOLD` | 16 |
| `DUAL_STACK_MAX_SKEW_EPOCHS` | 8 |
| `DUAL_STACK_MAX_SKEW_SECONDS` | 259200 (72 hours) |
| `AGREEMENT_ROTATION_MAX_SKEW_SECONDS` | 604800 (7 days) |
| `RECOVERY_DELAY_SECONDS` | 259200 |
| `CAPABILITY_MAX_TTL_SECONDS` | 31536000 (365 days) |

Implementations MUST use these values. They MUST NOT expose them as interop-breaking settings.

## Negative requirements

- MUST NOT use the identity Ed25519 key for X25519 Diffie–Hellman.
- MUST NOT sign raw DAG-CBOR or a raw 32-byte digest without the off-chain envelope.
- MUST NOT use JWT as the capability encoding.
- MUST NOT treat SIWS as a record signature.
- MUST NOT re-encrypt history on HPKE→MLS migration.
