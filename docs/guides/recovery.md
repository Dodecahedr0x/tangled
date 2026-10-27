# Offline host recovery

`tangled-recovery` captures and checks a stopped host's persistent `KnotEngine` state. It includes historical records, commits, Merkle tree blocks, owned blobs, indexes and persistent admission metadata. It restores into an empty directory and can scrub or repair a matching snapshot.

The archive contains private data. It is **unencrypted and unauthenticated**: checksums detect damage but do not authenticate its author or prove that it is current. Use a trusted source and protect the archive. Signing/decryption/MLS keys, sessions, importer databases, connector credentials and other application stores need separate backup and recovery procedures. Restoring ciphertext does not restore decryptability without its keys.

Stop the host before CLI backup, scrub, restore or repair. For the library API, hold the engine's write lock during `tangled::recovery::Backup::capture`.

```sh
cargo run -p tangled --bin tangled-recovery -- backup ./data DID ./host-backup.json
cargo run -p tangled --bin tangled-recovery -- verify ./host-backup.json
cargo run -p tangled --bin tangled-recovery -- restore ./host-backup.json ./empty-data DID
cargo run -p tangled --bin tangled-recovery -- scrub ./host-backup.json ./empty-data
cargo run -p tangled --bin tangled-recovery -- repair ./host-backup.json ./empty-data
```

Replace `DID` with the hosted identity. Backup refuses an existing archive path, and restore refuses a nonempty destination. On Unix, newly created archive/files use mode 0600 and directories mode 0700; existing directory permissions and ACLs are not normalized.

Exit status 0 means verification/operation success; status 1 means scrub found differences; status 2 means an operation failed or was refused. A successful `verify` checks integrity and local dependency/index closure. It does not validate every author's signature, establish snapshot freshness, or prove CRDT compact semantics. CID-shaped text is conservatively treated as a local dependency, so unresolved external references can prevent capture.

Repair requires the matching stopped snapshot. Valid changes to optional peer-discovery metadata also cause repair to refuse an older snapshot; this conservative policy avoids silently rolling state back. It refuses unexpected files, changed erasure fences, differing valid mutable metadata, and valid changed tip CIDs to avoid overwriting possible newer state. Missing mutable files are inherently ambiguous: use only a snapshot whose correspondence you can establish. This does not provide monotonic recovery after total state loss or protect newer erasure intent that disappeared with its authority state. Erased content must remain absent; refresh a stale signed head if it no longer matches the surviving tips before taking a new backup.

Repairs use synced temporary files and atomic per-file replacement, followed by another inventory check. The whole directory is not a transaction: interruption may leave a partial operation requiring another verified recovery attempt. Do not run it against a live or independently advanced store.

The initial format stores file bytes in JSON arrays, holds data in memory, and stages copies for validation. In the small reference fixture the JSON archive is about 3.85 times the payload size. These operations are not optimized for large stores. An external shared authorization authority, if introduced separately, would also need its own coordinated backup/recovery policy.
