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

Sync verifies the complete backup before making changes. An identical signed head is idempotent; a successor must extend the current signed commit history. Stale history, divergent forks, and a different owner are rejected. Make one installation's writes at a time, transfer its new backup, and sync before writing on the other installation. A stale backup can still be intentionally restored into a fresh directory; the command has no external trusted clock or remote latest-head oracle.

`{"action":"status"}` returns `owner`, current signed `head` (or null for a new empty identity), `records` (signed current tips, including tombstones), and local `blobs` (including schemas). Backup adds `output_path` and `encrypted_bytes`; verify, restore, and sync return the verified repository summary.

## Format and limits

The portable encrypted container is described by `docs/spec/schemas/personal.recovery.json`. Its owner DID is public metadata. Contents are JSON encoded then encrypted using the existing Tangled HPKE sealed-record helper (X25519 / HKDF-SHA256 / ChaCha20-Poly1305). A fresh random 16-byte salt and Argon2id v19 (19,456 KiB, two iterations, one lane) derive the recipient agreement seed from the passphrase. The associated data binds format version, KDF profile, and owner DID. Incorrect passphrases and tampering fail authentication.

Use a separate strong passphrase, 12–1024 UTF-8 bytes. The decoded contents are limited to 64 MiB; the file is limited to 128 MiB plus 4 KiB. CAR bytes and supplemental blobs are hex encoded, and referenced blob bytes may occur in both the CAR and supplemental inventory; consequently the practical raw-data ceiling is below 32 MiB. Backups fail explicitly when too large. No archive decompression, unbounded KDF tuning, or external credential lookup occurs.

Backup files and recovered identity files use mode 0600, and fresh restore directories use 0700 on Unix. Normal local repository data remains capability-gated rather than encrypted at rest; the local operator possesses the owner keys. Encryption protects the portable recovery file, not a compromised running process. Sensitive derived buffers are cleared on drop where supported; passphrase input and process/runtime copies are not guaranteed erased. Preserve backups and the passphrase separately. Interrupted destination persistence can leave a partial fresh directory, which should be inspected and removed or renamed before retrying; restoration never silently overwrites it.
