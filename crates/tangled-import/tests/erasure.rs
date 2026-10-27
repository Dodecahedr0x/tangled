use base64::{engine::general_purpose::STANDARD, Engine as _};
use tangled::{identity::Identity, knot::KnotEngine, record};
use tangled_import::{store::ArchiveStore, ImportBatch, ImportedFile, ImportedObject};

fn batch(source: &str, account: &str, text: &str, edit: i64) -> ImportBatch {
    ImportBatch {
        source: source.into(),
        account: account.into(),
        warnings: vec![],
        objects: vec![ImportedObject {
            id: "42".into(),
            kind: "message".into(),
            conversation: Some("7".into()),
            sender: None,
            timestamp: None,
            text: Some(text.into()),
            attachments: vec!["photo".into()],
            data: serde_json::json!({"edit_date":edit}),
        }],
        files: vec![ImportedFile {
            name: "photo".into(),
            media_type: "image/jpeg".into(),
            data_base64: STANDARD.encode(b"private photo"),
        }],
    }
}

#[test]
fn source_erasure_removes_history_and_disk_and_fences_old_backups() {
    let dir = tempfile::tempdir().unwrap();
    let engine_dir = dir.path().join("engine");
    let owner = Identity::from_seeds([151; 32], [152; 32]);
    let did = owner.did_string();
    let mut engine = KnotEngine::open(&engine_dir, &did).unwrap();
    engine.register_identity(&owner);
    let mut store = ArchiveStore::open(&dir.path().join("archive")).unwrap();
    let a = batch("telegram", "a", "old secret", 1);
    store
        .ingest(&did, &owner, &mut engine, "a", b"original a", &a)
        .unwrap();
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "a2",
            b"updated a",
            &batch("telegram", "a", "edited secret", 2),
        )
        .unwrap();
    let backup = store.backup(&did, &owner).unwrap();
    let a_records: Vec<_> = engine
        .stored
        .iter()
        .filter(|r| r.collection != "app.tangled.import.key")
        .map(|r| (r.cid.clone(), engine.blocks[&r.cid].clone()))
        .collect();
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "b",
            b"original b",
            &batch("telegram", "b", "keep", 1),
        )
        .unwrap();
    let connection_a = store.connect(&did, "telegram", "a").unwrap();
    let connection_b = store.connect(&did, "telegram", "b").unwrap();
    store.remember_signer(&did, &owner).unwrap();
    let result = store
        .erase_source(&did, &owner, &mut engine, "telegram", "a")
        .unwrap();
    assert!(result.records > 0);
    assert_eq!(store.objects(&did, &owner).unwrap().len(), 1);
    assert_eq!(store.objects(&did, &owner).unwrap()[0].account, "b");
    assert!(store
        .authorize_ingest(&connection_a.connection.id, &connection_a.token)
        .is_err());
    assert!(store
        .authorize_ingest(&connection_b.connection.id, &connection_b.token)
        .is_ok());
    assert!(store.connector_signer(&did).is_ok());
    assert!(store.restore(&did, &owner, &backup).is_err());
    store
        .ingest(&did, &owner, &mut engine, "replay", b"replayed raw", &a)
        .unwrap();
    assert_eq!(store.objects(&did, &owner).unwrap().len(), 1);
    drop(engine);
    drop(store);
    let reopened = KnotEngine::open(&engine_dir, &did).unwrap();
    for (cid, bytes) in a_records {
        assert!(!reopened.stored.iter().any(|r| r.cid == cid));
        assert!(!reopened.blocks.contains_key(&cid));
        assert!(!all_files(&engine_dir).iter().any(|b| b == &bytes));
    }
    let reopened = ArchiveStore::open(&dir.path().join("archive")).unwrap();
    assert_eq!(reopened.erasure_generation(&did).unwrap(), 1);
    assert_eq!(reopened.objects(&did, &owner).unwrap().len(), 1);
}

fn all_files(path: &std::path::Path) -> Vec<Vec<u8>> {
    let mut files = vec![];
    for item in std::fs::read_dir(path).unwrap() {
        let path = item.unwrap().path();
        if path.is_dir() {
            files.extend(all_files(&path));
        } else {
            files.push(std::fs::read(path).unwrap());
        }
    }
    files
}

#[test]
fn whole_erasure_preserves_native_public_and_other_owners() {
    let dir = tempfile::tempdir().unwrap();
    let engine_dir = dir.path().join("engine");
    let owner = Identity::from_seeds([161; 32], [162; 32]);
    let other = Identity::from_seeds([163; 32], [164; 32]);
    let did = owner.did_string();
    let mut engine = KnotEngine::open(&engine_dir, &did).unwrap();
    engine.register_identity(&owner);
    engine.register_identity(&other);
    let mut store = ArchiveStore::open(&dir.path().join("archive")).unwrap();
    for id in [&owner, &other] {
        store
            .ingest(
                &id.did_string(),
                id,
                &mut engine,
                "raw",
                b"raw",
                &batch("telegram", "a", "secret", 1),
            )
            .unwrap();
        store.remember_signer(&id.did_string(), id).unwrap();
    }
    let unsigned = serde_json::json!({"v":1,"type":tangled::cid::Cid::dag_cbor(b"app.tangled.post").to_string(),"collection":"app.tangled.post","rkey":"public","author":did,"ts":1,"prev":[],"vis":"public","body":{"text":"keep native"}});
    let native = record::sign_json_record(owner.signing_key(), &unsigned).unwrap();
    let native = engine.ingest_record(native.json).unwrap().cid.unwrap();
    let backup = store.backup(&did, &owner).unwrap();
    store.erase_all(&did, &owner, &mut engine).unwrap();
    assert!(store.backup(&did, &owner).unwrap().archive_key.is_none());
    assert!(store.connector_signer(&did).is_err());
    assert!(store.restore(&did, &owner, &backup).is_err());
    assert_eq!(store.objects(&other.did_string(), &other).unwrap().len(), 1);
    assert!(engine.blocks.contains_key(&native));
    assert!(!engine
        .stored
        .iter()
        .any(|r| r.did == did && r.collection.starts_with("app.tangled.import.")));
    drop(engine);
    drop(store);
    let reopened = KnotEngine::open(&engine_dir, &did).unwrap();
    assert!(reopened.blocks.contains_key(&native));
    assert!(!reopened
        .stored
        .iter()
        .any(|r| r.did == did && r.collection.starts_with("app.tangled.import.")));
}

#[test]
fn erasure_denies_foreign_identity_and_retires_old_key_even_if_generation_is_edited() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([171; 32], [172; 32]);
    let foreign = Identity::from_seeds([173; 32], [174; 32]);
    let did = owner.did_string();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut engine = KnotEngine::hosting(&did);
    engine.register_identity(&owner);
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "raw",
            b"raw",
            &batch("telegram", "a", "secret", 1),
        )
        .unwrap();
    let mut old = store.backup(&did, &owner).unwrap();
    assert!(store.erase_all(&did, &foreign, &mut engine).is_err());
    assert_eq!(store.erasure_generation(&did).unwrap(), 0);
    store.erase_all(&did, &owner, &mut engine).unwrap();
    old.erasure_generation = store.erasure_generation(&did).unwrap();
    assert!(store.check_restore_erasure(&did, &old).is_err());
    assert!(store.restore(&did, &owner, &old).is_err());
}

#[test]
fn erased_record_replay_and_interrupted_disk_cleanup_are_fenced_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let engine_dir = dir.path().join("engine");
    let owner = Identity::from_seeds([175; 32], [176; 32]);
    let did = owner.did_string();
    let mut store = ArchiveStore::open(&dir.path().join("archive")).unwrap();
    let mut engine = KnotEngine::open(&engine_dir, &did).unwrap();
    engine.register_identity(&owner);
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "raw",
            b"raw",
            &batch("telegram", "a", "secret", 1),
        )
        .unwrap();
    let erased = engine
        .stored
        .iter()
        .find(|r| r.collection == "app.tangled.import.object")
        .unwrap()
        .clone();
    let bytes = engine.blocks[&erased.cid].clone();
    store.erase_all(&did, &owner, &mut engine).unwrap();
    assert!(engine.ingest_record(erased.record.clone()).is_err());
    drop(engine);
    // Simulate a process interruption after writing the durable fence but before unlinking.
    let safe = |s: &str| s.replace(['/', '\\', ':'], "_");
    let path = engine_dir
        .join("repos")
        .join(safe(&did))
        .join("collections")
        .join(safe(&erased.collection))
        .join(safe(&erased.rkey));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join(format!("{}.json", erased.cid)),
        serde_json::to_vec(&erased).unwrap(),
    )
    .unwrap();
    std::fs::write(engine_dir.join("blocks").join(&erased.cid), bytes).unwrap();
    let mut reopened = KnotEngine::open(&engine_dir, &did).unwrap();
    assert!(!reopened.stored.iter().any(|r| r.cid == erased.cid));
    assert!(!reopened.blocks.contains_key(&erased.cid));
    assert!(!path.join(format!("{}.json", erased.cid)).exists());
    assert!(reopened.ingest_record(erased.record).is_err());
}

#[test]
fn portable_post_erasure_backup_restores_into_fresh_host() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([177; 32], [178; 32]);
    let did = owner.did_string();
    let mut store = ArchiveStore::open(&dir.path().join("source")).unwrap();
    let mut engine = KnotEngine::hosting(&did);
    engine.register_identity(&owner);
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "a",
            b"a",
            &batch("telegram", "a", "erase", 1),
        )
        .unwrap();
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "b",
            b"b",
            &batch("twitter", "b", "keep", 1),
        )
        .unwrap();
    store
        .erase_source(&did, &owner, &mut engine, "telegram", "a")
        .unwrap();
    let backup = store.backup(&did, &owner).unwrap();
    let mut destination = ArchiveStore::open(&dir.path().join("destination")).unwrap();
    destination.restore(&did, &owner, &backup).unwrap();
    assert_eq!(destination.erasure_generation(&did).unwrap(), 1);
    assert_eq!(destination.objects(&did, &owner).unwrap().len(), 1);
}

#[test]
fn failed_disk_erasure_is_retryable_without_leaving_old_record_files() {
    let dir = tempfile::tempdir().unwrap();
    let engine_dir = dir.path().join("engine");
    let owner = Identity::from_seeds([185; 32], [186; 32]);
    let did = owner.did_string();
    let mut store = ArchiveStore::open(&dir.path().join("archive")).unwrap();
    let mut engine = KnotEngine::open(&engine_dir, &did).unwrap();
    engine.register_identity(&owner);
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "raw",
            b"raw",
            &batch("telegram", "a", "secret", 1),
        )
        .unwrap();
    let erased = engine
        .stored
        .iter()
        .find(|r| r.collection == "app.tangled.import.object")
        .unwrap()
        .clone();
    let block = engine_dir.join("blocks").join(&erased.cid);
    std::fs::remove_file(&block).unwrap();
    std::fs::create_dir(&block).unwrap(); // Force a real unlink error after the fence is durable.
    assert!(store.erase_all(&did, &owner, &mut engine).is_err());
    assert_eq!(store.objects(&did, &owner).unwrap().len(), 1);
    let safe = |s: &str| s.replace(['/', '\\', ':'], "_");
    let path = engine_dir
        .join("repos")
        .join(safe(&did))
        .join("collections")
        .join(safe(&erased.collection))
        .join(safe(&erased.rkey))
        .join(format!("{}.json", erased.cid));
    std::fs::write(&path, serde_json::to_vec(&erased).unwrap()).unwrap();
    std::fs::remove_dir(&block).unwrap(); // Repair the filesystem failure.
    store.erase_all(&did, &owner, &mut engine).unwrap();
    assert!(!path.exists());
    assert!(store.backup(&did, &owner).unwrap().archive_key.is_none());
    let reopened = KnotEngine::open(&engine_dir, &did).unwrap();
    assert!(!reopened
        .stored
        .iter()
        .any(|r| r.did == did && r.collection.starts_with("app.tangled.import.")));
}

#[test]
fn shared_foreign_owner_blob_keeps_its_copy_and_drops_erased_owners_index() {
    let dir = tempfile::tempdir().unwrap();
    let engine_dir = dir.path().join("engine");
    let owner = Identity::from_seeds([187; 32], [188; 32]);
    let foreign = Identity::from_seeds([189; 32], [190; 32]);
    let did = owner.did_string();
    let mut engine = KnotEngine::open(&engine_dir, &did).unwrap();
    engine.register_identity(&owner);
    let mut store = ArchiveStore::open(&dir.path().join("archive")).unwrap();
    store
        .ingest(
            &did,
            &owner,
            &mut engine,
            "raw",
            b"raw",
            &batch("telegram", "a", "secret", 1),
        )
        .unwrap();
    let ((_, cid), bytes) = engine.blobs.iter().next().unwrap();
    let (cid, bytes) = (cid.clone(), bytes.clone());
    engine
        .blobs
        .insert((foreign.did_string(), cid.clone()), bytes.clone());
    engine.persist().unwrap();
    store.erase_all(&did, &owner, &mut engine).unwrap();
    assert!(!engine.blobs.contains_key(&(did.clone(), cid.clone())));
    assert_eq!(engine.blobs[&(foreign.did_string(), cid.clone())], bytes);
    assert_eq!(engine.blocks[&cid], bytes);
    let reopened = KnotEngine::open(&engine_dir, &did).unwrap();
    assert!(!reopened.blobs.contains_key(&(did, cid.clone())));
    assert_eq!(reopened.blobs[&(foreign.did_string(), cid.clone())], bytes);
    assert_eq!(reopened.blocks[&cid], bytes);
}

#[test]
fn source_erasure_clears_staged_uploads_even_when_account_hint_differs() {
    use tangled_import::store::uploads::UploadRequest;
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([191; 32], [192; 32]);
    let did = owner.did_string();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut engine = KnotEngine::hosting(&did);
    let request = |source: &str| UploadRequest {
        source: source.into(),
        account: "export account hint".into(),
        filename: "export.json".into(),
        total: 6,
        sha256: None,
    };
    let upload = store.create_upload(&did, request("telegram")).unwrap();
    store.append_upload(&did, &upload.id, 0, b"secret").unwrap();
    let other = store.create_upload("foreign", request("telegram")).unwrap();
    store
        .append_upload("foreign", &other.id, 0, b"secret")
        .unwrap();
    let other_source = store.create_upload(&did, request("twitter")).unwrap();
    store
        .append_upload(&did, &other_source.id, 0, b"secret")
        .unwrap();
    let receipt = store
        .erase_source(&did, &owner, &mut engine, "telegram", "native account")
        .unwrap();
    assert_eq!(receipt.uploads, 1);
    assert!(store.upload_status(&did, &upload.id).is_err());
    assert_eq!(
        store.upload_status("foreign", &other.id).unwrap().received,
        6
    );
    let db = rusqlite::Connection::open(dir.path().join("imports.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM upload_sessions WHERE owner=?1 AND source='twitter'",
            [&did],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM upload_chunks WHERE id=?1",
            [&other_source.id],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM upload_chunks WHERE id=?1",
            [&upload.id],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn different_source_uploads_resume_after_erasure_without_holding_quota() {
    use tangled_import::store::uploads::UploadRequest;
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([193; 32], [194; 32]);
    let did = owner.did_string();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut engine = KnotEngine::hosting(&did);
    let request = |source: &str| UploadRequest {
        source: source.into(),
        account: "account".into(),
        filename: "export.json".into(),
        total: 12,
        sha256: None,
    };
    let mut preserved = Vec::new();
    for _ in 0..7 {
        let upload = store.create_upload(&did, request("whatsapp")).unwrap();
        store.append_upload(&did, &upload.id, 0, b"secret").unwrap();
        preserved.push(upload.id);
    }
    let erased = store.create_upload(&did, request("telegram")).unwrap();
    store
        .erase_source(&did, &owner, &mut engine, "telegram", "account")
        .unwrap();
    assert!(store.upload_status(&did, &erased.id).is_err());
    drop(store);
    let mut reopened = ArchiveStore::open(dir.path()).unwrap();
    for id in preserved {
        assert_eq!(reopened.upload_status(&did, &id).unwrap().received, 6);
        reopened.append_upload(&did, &id, 6, b"secret").unwrap();
        assert_eq!(reopened.upload_bytes(&did, &id).unwrap().1, b"secretsecret");
        reopened.cancel_upload(&did, &id).unwrap();
    }
    // Every previous slot is usable immediately; preserved sessions did not become
    // inaccessible generation-stale entries that exhaust the seven-day quota.
    for _ in 0..8 {
        reopened.create_upload(&did, request("whatsapp")).unwrap();
    }
    assert!(reopened.create_upload(&did, request("whatsapp")).is_err());
}
