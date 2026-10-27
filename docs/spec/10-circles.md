---
title: Circles
slug: circles
order: 10
group: auth
---

# Circles

A circle is a group. From the user’s point of view the operations are: create, add member, remove member, send, leave. There is no “upgrade group,” no re-invite, no new circle id, no decryption gap, and no prompt. Hybrid Public Key Encryption (HPKE) versus Messaging Layer Security (MLS) MUST NOT appear in any client API that authors or readers call.

## Definition record

Collection `sys.circle`, rkey = TID (this TID is the circle id). The author DID plus rkey form the circle DID for resource URIs: `tangled://circle/did:sol:<author>/<rkey>` in prose, stored as `circle: did:sol:…#<rkey>`.

```
Circle = {
  v: 1,
  name: text,
  members: [* did],           ; omitted or empty when hideMembers
  policy: {
    invite: "owner" / "member",
    write: "member",
    history_share: bool,      ; default false
    hideMembers: bool         ; default false
  },
  epoch: uint,                ; current N
  distributor: "hpke" / "mls" / "dual",
  ? mlsGroup: bstr,           ; MLS group context hash, once MLS has been used
  threshold: 16               ; MUST equal CIRCLE_MLS_THRESHOLD
}
```

`distributor` is diagnostic for implementations. Clients MUST NOT show it. `threshold` is written so a reader of the record can detect a non-conforming writer; writers MUST emit `16`.

`sys.circle` MUST use `vis: public` or `vis: cap`. It MUST NOT use `vis: sealed` (policy and membership grants have to be verifiable without the epoch secret). `hideMembers: true` SHOULD use `vis: cap` so looms do not ingest an empty-but-named group if the owner wants it unlisted. Wrap and MLS log records MUST use `vis: sealed` and MUST set envelope `circle`.

## Epoch secret

Application payloads in a circle are AEAD’d with keys derived from `epoch_secret[N]` ([Cryptography](./05-cryptography.md#epoch-application-keys)). How members obtain `N` is the only thing that changes with size.

| Distributor | When (automatic) | Mechanism |
| --- | --- | --- |
| Small-group `hpke` | `len(members) ≤ 16` **and** the circle has never used MLS | HPKE-wrap of `epoch_secret[N]` to each member’s X25519, stored in `sys.circle.wrap` |
| Large-group `mls` | `len(members) > 16`, **or** the circle has already used MLS | MLS exporter secret with label `tangled.epoch` **is** `epoch_secret[N]` |
| Dual-stack `dual` | The single transition after HPKE→MLS grow | Both of the above for epoch `N+1` |

## Write patterns

**Annotation (default).** A member writes a `vis: sealed` record in **their** repository with envelope `circle` set, `body.alg = "epoch-aead"`, `epoch = N`, `ref` optional. The record is portable. The circle knot does not become a god. Other members fetch it with a **membership grant** (`circle/read`), not with pairwise `repo/read` on every sender. Cost: each member pulls every other member (N−1).

**Shared repo.** The circle has a dedicated Solana key (a DID the owner creates) whose repository is the shared object. Used for wikis, datasets, and optional high-traffic chat. One subscribe. Specified below.

Both patterns use the same epoch secret. Users do not pick a pattern as a cryptographic mode; clients pick it per application.

## Membership grant

When a DID is added to a circle, the owner MUST issue a capability to that DID:

- `cmd`: `circle/read`
- `res`: `tangled://circle/<owner-did>/<circle-rkey>`
- `exp` within `CAPABILITY_MAX_TTL_SECONDS`

This is the **membership grant**. The adder MAY also issue `circle/invite` if `policy.invite` is `"member"`. Application senders MUST set envelope `circle` on circle-scoped records. Knots that advertise `knot.group` or `knot.sealed` MUST honor `circle/read` as specified in [Capabilities](./09-capabilities.md#circleread-verification).

On remove or leave, the owner MUST publish `sys.revocation` for that member’s membership grant(s) **and** advance the epoch. Revocation stops fetch; the epoch advance stops read of new payloads. A revoked member MAY still hold ciphertext they already fetched and old epoch secrets.

## Shared-repo pattern

The owner creates a Solana keypair whose DID `W` owns the MST. `W` is hosted on a knot. Records in that repo have `author` = `W`. Collaborators hold attenuated `repo/read`, `repo/write`, and `blob/*` capabilities; they sign records with their device key (`signedBy` + `proof`). The hosting knot MUST sign commits with a key of `W` (identity key or a device key with `repo/write`). That is the same hosting arrangement as any identity that does not keep its identity key on the knot.

`vis: cap` is the class for a wiki the host is allowed to search. `vis: sealed` plus envelope `circle` is the class for a confidential shared object. Collaborators cannot rotate `W`; the owner holds that key. Example collection: `pub.tangled.page`, rkey = slug.

The owner MAY instead host the wiki as a collection prefix on their own Decentralized Identifier (DID). Then `author` is the owner, and collaborators are device-like writers of that collection. A dedicated DID isolates a compromised collaborator capability from the owner’s other collections.

## Create

Owner generates `epoch_secret[0]` (32 cryptographically random bytes), writes `Circle` with `distributor: "hpke"`, `epoch: 0`, `members: [owner]`, and a wrap to the owner. MLS is not involved.

## Add member

User action: add a DID.

Implementation:

1. Authorize via `circle/invite` policy.
2. Linearize on the circle commit log (one membership change at a time).
3. Issue (or refresh) the membership grant to the new member.
4. If `distributor` is `hpke` and `len(members)+1 ≤ 16`: new epoch `N+1`, new random `epoch_secret[N+1]`, HPKE-wrap to every current member including the new one. Old wraps remain so history decrypts.
5. If `distributor` is `hpke` and `len(members)+1 > 16`: perform **seamless grow** below. This is still “add member” to the user.
6. If `distributor` is `mls` or `dual`: MLS Add + Commit, Welcome for the new member. If the new member is offline, the adder MUST use a current `sys.mls.keypackage` from that member’s repository ([KeyPackage records](#keypackage-records)). If `dual`, also HPKE-wrap `epoch_secret[N+1]` to members who have not processed Welcome.

If `history_share` is true, the owner (or the adder, if they hold old secrets) additionally wraps epoch secrets `0..N` to the new member. If false, the new member cannot decrypt pre-join records. `history_share` is an explicit policy, not a side effect of size migration.

## Seamless grow (HPKE → MLS)

The `addMember` that first exceeds 16 members MUST:

1. Create an MLS group whose members are exactly the current circle including the new member. Credential for each member is that member’s 32-byte Solana public key. Ciphersuite `0x0001`. Each leaf HPKE public key MUST be that member’s Tangled X25519 `agreement` key. RFC 9420 requires KeyPackage `init_key` ≠ leaf; Welcome is sealed to `init_key`, not to `agreement`.
2. Publish a Welcome for every current member as `sys.circle.mls` records, `vis: sealed` to that member (HPKE 1:1 of the Welcome bytes). Offline members pick them up on next sync. No accept dialog.
3. Advance to epoch `N+1`. Set `epoch_secret[N+1]` to the MLS exporter secret (label `tangled.epoch`, context `uint64be(N+1)`, 32 bytes).
4. Set `distributor: "dual"`. HPKE-wrap `epoch_secret[N+1]` to every member who has not been observed to process Welcome (initially: everyone). Application senders encrypt **once**, to the epoch AEAD key. Laggards decrypt via the wrap they already understand; caught-up members decrypt via MLS.
5. Dual-stack ends when every member known at the transition has a Welcome receipt record (`sys.circle.mls` body `{type:"receipt", epoch, member}`) **or** `DUAL_STACK_MAX_SKEW_EPOCHS` epochs have passed **or** `DUAL_STACK_MAX_SKEW_SECONDS` have elapsed, whichever first. Then `distributor: "mls"`. Knots MUST stop writing new HPKE wraps for this circle except as required by `hideMembers` debugging flags (none in v1).
6. Membership changes during dual-stack are applied as MLS epochs, never as a new HPKE-only epoch.

## Never downgrade

Once a circle has published an MLS epoch (`mlsGroup` is set), it stays on MLS even if membership later drops to two. Implementations MUST NOT return to `hpke`. Ping-pong is not seamless. MLS is correct for two members.

## Agreement rotation

Rotating `sys/id.agreement` (with or without rotating `pubkey`) changes the X25519 key that Hybrid Public Key Encryption (HPKE) wraps and Messaging Layer Security (MLS) leaves use. Moving a knot does **not**. Rotating only a device key does **not**. HPKE versus MLS MUST NOT appear as a user-facing “upgrade group,” “re-wrap,” or “re-invite” prompt. The rotating member’s client performs the steps in this section automatically after it publishes the new `sys/id`.

### Who initiates

The rotating member’s client MUST initiate re-wrap. Other members (usually the circle owner, on the next epoch advance) MUST read the current `agreement` from that member’s `sys/id` when wrapping. They MUST NOT wait for a confirmation dialog.

### MUST path

After a new `sys/id` whose `agreement` changed:

1. The rotating client MUST HPKE-wrap every `epoch_secret[N]` it still needs — the current epoch, and prior epochs if it retains history or `history_share` is true — to the **new** agreement key. Info string remains `tangled.hpke.epoch.v1`.
2. It MUST publish those wraps as `sys.circle.wrap` records with envelope `circle` set and `vis: sealed`. It SHOULD write them in **its own** repository so its other devices can pull without `repo/write` on the owner. The owner SHOULD copy observed self-wraps into the owner’s circle log.
3. If `distributor` is `mls` or `dual`, the rotating client MUST publish an MLS Update/Commit whose leaf HPKE key is the new agreement public key. If `pubkey` also changed, the MLS credential MUST be the new 32-byte Solana public key. Offline circle members pick this up from the log; it is not a re-invite.
4. The rotating client SHOULD `tangled.sync.notify` the circle owner’s knot after the new `sys/id` commit.

Identity-key rotation without a new `agreement` is forbidden: writers MUST emit a new X25519 key together with a new `pubkey` ([Identifiers](./04-identifiers.md#rotation)).

### Old agreement window

Wraps to the previous agreement key remain valid for decryption until `AGREEMENT_ROTATION_MAX_SKEW_SECONDS` after the new `sys/id` record’s `ts`. During that window:

- Devices of the rotating member that have not synced MAY decrypt with the old agreement private key.
- An owner who advances the epoch SHOULD wrap to **both** agreement keys if it has not observed a wrap (or MLS receipt) for the new key.
- After the window, owners MUST wrap only to the current `sys/id.agreement`. The rotating client SHOULD discard the old agreement private key once every device it controls holds the new wraps.

An offline **device of the rotator** that returns after the window without new wraps MUST obtain them from another of that member’s devices (the online device re-publishes). An offline **circle member** is unaffected except that they will observe the new `sys/id` on next pull.

### Device-key-only rotation

Revoke the old device’s capabilities; issue new ones to the new device key. `sys/id.pubkey` and `sys/id.agreement` MUST NOT change. Clients MUST NOT re-wrap epoch secrets and MUST NOT publish MLS Updates solely because a device key changed.

## Remove member and leave

User action: remove or leave.

Advance epoch. Under `hpke`, generate a new secret and wrap to remaining members only. Under `mls` or `dual`, MLS Remove + Commit. The removed member loses `epoch_secret[N+1]` onward (forward secrecy). They keep secrets they already have. There is no extra “key rotation ceremony” in the UI.

## Send

Encrypt the application body with the current epoch AEAD. Recipients decrypt with `epoch_secret[epoch]` from wraps or MLS. If a sender is lagging (still on `N-1` during dual-stack), receivers MUST accept epochs in `[current - DUAL_STACK_MAX_SKEW_EPOCHS, current]`. Older epochs MUST be rejected for new sends; they remain valid for historical records.

Senders using the annotation pattern MUST set envelope `circle`. Clients SHOULD `subscribe` or poll each current member’s knot; they MAY `tangled.sync.notify` after send. There is no circle-wide inbox POST.

## KeyPackage records

Messaging Layer Security (MLS) Add needs a KeyPackage. A member who may be added while offline MUST publish one in **their** repository so an adder can Welcome without a round-trip. This is still “add member.” Clients MUST NOT expose KeyPackage publish, Hybrid Public Key Encryption (HPKE), or MLS as a user-facing mode.

- collection `sys.mls.keypackage`
- rkey = Timestamp Identifier (TID)
- `vis`: `public` or `cap`
- body `{ type: "keypackage", exp: uint, payload: bstr }`

`payload` MUST be the RFC 9420 TLS-serialized KeyPackage. `init_key` MUST differ from the leaf / `agreement` key. `exp` MUST be Unix seconds. An adder MUST ignore a package whose `exp` is outside the capability clock-skew window ([Capabilities](./09-capabilities.md#invocation)). When adding an offline member, the adder MUST Welcome to a current unexpired package. The new member’s client processes Welcome on next sync. No accept dialog.

## Offline members

Welcomes, wraps, and KeyPackage records are records. Sync delivers them. An offline member who returns during dual-stack reads via HPKE wrap, processes Welcome, and continues on MLS. An offline member who returns after dual-stack MUST still have a Welcome in the log; if the Welcome was GC’d (MUST NOT before max-skew), the member is re-Welcomed by any current member — still not a user-facing re-invite: the client does it on sync. An adder who lacks a live KeyPackage MUST fetch `sys.mls.keypackage` from the new member’s repository.

## Forced-upgrade denial of service

Spam-adding members to trigger MLS is a policy problem, not a dialog. Circles SHOULD rate-limit invites (`max_adds_per_hour` in `policy`, default 30). Clients MUST NOT ask the user to “confirm MLS.”

## Conformance

`docs/spec/vectors/circle-grow-dual-stack.json` MUST pass: 16 members on HPKE, 17th added, one member offline throughout, pre-migration records still decrypt, circle id unchanged, no client API flag. Shrink 20→5 stays MLS. Add and remove during dual-stack linearize. Agreement rotation MUST follow [Agreement rotation](#agreement-rotation); `docs/spec/vectors/circle-read-auth.json` covers membership-grant authorization, not wrap ciphertext. `docs/spec/vectors/mls-welcome-commit.json` covers wrapping fields and exporter parameters; RFC 9420 payload bytes are produced by a conforming MLS stack (Welcome `init_key` ≠ leaf/`agreement`).
