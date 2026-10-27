---
title: Personal device continuity
slug: personal-devices
description: Verify encrypted recovery copies and transfer signed owner state explicitly.
order: 15
---

# Own-device continuity

The `tangled-personal devices` command transfers a complete owner repository using an encrypted file. Recovery works in a fresh directory without the original installation: the file includes both owner identity seeds, the signed repository CAR, and all local owner attachment bytes, including blobs not yet referenced by a record. Restored records keep their Tangled URIs, CIDs, and owner DID. A restored installation can sign subsequent writes.

This first version uses explicit file transfer. It has no mobile binary, network transport, automatic background sync, device discovery, or divergent-history merge. Each recovered installation holds the same full owner keys. Individual copies of those keys cannot be revoked. Device pairing and capability delegation are separate workflows; this command does not pretend a metadata device list can revoke an owner replica.

## Commands

Run `cargo run -p tangled-personal -- --data ./personal devices` and provide one JSON object on standard input. Passphrases are JSON input fields; do not place them in command flags or shell history. Store any input file containing a passphrase privately and remove it when finished. Outputs contain summaries and paths, never passphrases, plaintext records, or owner seeds.

Create a backup, refusing to overwrite an existing file:

```json
{"action":"backup","output_path":"/private/backup/owner.json","passphrase":"a separate long recovery passphrase"}
```

Verify decryption, identity correspondence, signed repository admission, and attachment CIDs without changing either the current store or a destination:

```json
{"action":"verify","input_path":"/private/backup/owner.json","passphrase":"a separate long recovery passphrase"}
```

Restore into a fresh directory whose parent exists. The destination must not already exist, even if empty. This does not change the current store:

```json
{"action":"restore","input_path":"/private/backup/owner.json","destination":"/private/restored-owner","passphrase":"a separate long recovery passphrase"}
```

Open the restored directory in a later command with `--data /private/restored-owner`. The original store is unnecessary for recovery; the CLI's current store is only an invocation context. Copy the encrypted file yourself to another device and use the same restore action there.

Import a later backup into an existing installation of the same owner:

```json
{"action":"sync","input_path":"/private/backup/newer-owner.json","passphrase":"a separate long recovery passphrase"}
```

Sync verifies the complete backup before making changes. An identical signed head is verified again and can repair missing materialized records or add attachment bytes; a successor must extend the current signed commit history. Stale history, divergent forks, and a different owner are rejected. Make one installation's writes at a time, transfer its new backup, and sync before writing on the other installation. A stale backup can still be intentionally restored into a fresh directory; the command has no external trusted clock or remote latest-head oracle.

`{"action":"status"}` returns `owner`, current signed `head` (or null for a new empty identity), `records` (signed current tips, including tombstones), and local `blobs` (including schemas). Backup adds `output_path` and `encrypted_bytes`; verify, restore, and sync return the verified repository summary.

## Verified imports and durable publication

Owner imports admit only record blocks referenced by the current or historical signed commit MSTs. Every commit, tree node, record CID, envelope signature, and tree-key/envelope mapping is checked before publication. Missing historical records or tree nodes reject the import. Current tips come from the signed root tree independently of CAR block order. Unrelated blocks remain opaque retained data and cannot create records or revoke capabilities. Unknown collection/body extensions and raw attachments remain retainable. Direct CAR imports are bounded to 64 MiB and one million commit/tree/entry traversal steps, with iterative commit/tree traversal.

Import and sync stage a complete engine snapshot in a private `.personal-transaction` directory before changing live files. The staged snapshot is reopened and compared with the candidate before publication; filesystem path collisions that cannot faithfully represent distinct keys reject the import. A durable manifest and saved prior entries allow recovery under the Store's exclusive lock. Errors during staging leave the old repository untouched; errors during publication roll back before returning. On reopening, interrupted uncommitted publication restores the prior snapshot; a committed journal is cleaned up. Owner identity files and the `personal.lock` inode never move. Files and directories in the staged snapshot use 0600 and 0700 on Unix. Temporary snapshot space can approach twice the repository size.

This recovery contract applies to the personal Store's import/sync/restore operations. It requires filesystem rename and fsync support and exclusive Store access. Other processes must respect `personal.lock`; a generic KnotEngine reader that bypasses it can observe publication in progress. Ordinary record/blob writes retain the core engine's existing persistence behavior. The reference filesystem backend escapes colon characters as underscores in paths: a single colon-bearing key is valid, but a persistent Store rejects a write before mutation if its escaped path collides with another current key (including a tombstone). Import staging likewise rejects snapshots with unrepresentable collisions. This backend limitation does not change protocol key validity. Do not manually remove a pending journal; reopen the Store to recover it.

## Format and limits

The portable encrypted container is described by `docs/spec/schemas/personal.recovery.json`. Its owner DID is public metadata. Contents are JSON encoded then encrypted using the existing Tangled HPKE sealed-record helper (X25519 / HKDF-SHA256 / ChaCha20-Poly1305). A fresh random 16-byte salt and Argon2id v19 (19,456 KiB, two iterations, one lane) derive the recipient agreement seed from the passphrase. The associated data binds format version, KDF profile, and owner DID. Incorrect passphrases and tampering fail authentication.

Use a separate strong passphrase, 12–1024 UTF-8 bytes. The decoded contents are limited to 64 MiB; the file is limited to 128 MiB plus 4 KiB. CAR bytes and supplemental blobs are hex encoded, and referenced blob bytes may occur in both the CAR and supplemental inventory; consequently the practical raw-data ceiling is below 32 MiB. Backups fail explicitly when too large. No archive decompression, unbounded KDF tuning, or external credential lookup occurs.

Backup files and recovered identity files use mode 0600, and fresh restore directories use 0700 on Unix. Normal local repository data remains capability-gated rather than encrypted at rest; the local operator possesses the owner keys. Encryption protects the portable recovery file, not a compromised running process. Sensitive derived buffers are cleared on drop where supported; passphrase input and process/runtime copies are not guaranteed erased. Preserve backups and the passphrase separately. An interrupted fresh restore can leave its newly created destination and owner identity in place. Reopening that directory recovers its journal; restoration never silently overwrites an existing destination.
