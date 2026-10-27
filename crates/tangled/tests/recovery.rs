//! Behavioral regression coverage for offline host recovery.
use std::{collections::BTreeSet, fs, path::PathBuf};
use tangled::{
    http::{KnotRequest, Method},
    record,
    recovery::Backup,
    Cid, Identity, KnotEngine,
};

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tangled-recovery-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(source: &std::path::Path, destination: &std::path::Path) -> (usize, u64) {
    let mut files = 0;
    let mut bytes = 0;
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir(&target).unwrap();
            let (f, b) = copy_tree(&entry.path(), &target);
            files += f;
            bytes += b;
        } else {
            bytes += fs::copy(entry.path(), target).unwrap();
            files += 1;
        }
    }
    (files, bytes)
}

fn fixture() -> (KnotEngine, String) {
    let id = Identity::from_seeds([71; 32], [72; 32]);
    let did = id.did_string();
    let mut e = KnotEngine::hosting(&did);
    e.register_identity(&id);
    let blob = b"recoverable raw attachment".to_vec();
    let cid = Cid::raw(&blob).to_string();
    e.blobs.insert((did.clone(), cid.clone()), blob);
    e.blob_types
        .insert((did.clone(), cid.clone()), "text/plain".into());
    let mut prev = Vec::<String>::new();
    for i in 0..12 {
        let body = if i == 11 {
            serde_json::json!({"compact":1,"included":prev,"snapshot":{"text":"fixture"},"blob":cid})
        } else {
            serde_json::json!({"text":format!("version {i}"),"blob":cid})
        };
        let unsigned = serde_json::json!({"v":1,"collection":"pub.tangled.post","rkey":"history","author":did,"vis":if i == 10 {"sealed"} else {"public"},"prev":prev,"body":body});
        let signed = record::sign_json_record(id.signing_key(), &unsigned).unwrap();
        prev.push(signed.cid.to_string());
        e.ingest_record(signed.json).unwrap();
    }
    (e, did)
}

#[test]
fn restore_empty_store_preserves_history_blobs_heads_and_restart() {
    let (e, did) = fixture();
    let backup = Backup::capture(&e).unwrap();
    let encoded = backup.to_bytes().unwrap();
    let backup = Backup::from_bytes(&encoded).unwrap();
    let dir = Dir::new();
    let restored = backup.restore(&dir.0, &did).unwrap();
    assert_eq!(restored.stored.len(), 12);
    assert_eq!(restored.blocks, e.blocks);
    assert_eq!(restored.blobs, e.blobs);
    assert_eq!(restored.heads[&did].commit, e.heads[&did].commit);
    assert_eq!(restored.tips, e.tips);
    assert_eq!(restored.blob_types, e.blob_types);
    assert!(restored.signing_keys.is_empty());
    assert!(backup.scrub(&dir.0).unwrap().is_clean());
    let reopened = KnotEngine::open(&dir.0, &did).unwrap();
    assert_eq!(reopened.stored.len(), 12);
    assert_eq!(reopened.blocks, e.blocks);
    assert!(backup.restore(&dir.0, &did).is_err());
}

#[test]
fn missing_or_corrupt_commit_mst_record_and_blob_fail_capture() {
    let (e, did) = fixture();
    let head = &e.heads[&did];
    for cid in [&head.commit, &head.data, &e.stored[0].cid] {
        let mut lost = e.clone();
        lost.blocks.remove(cid);
        assert!(Backup::capture(&lost).is_err(), "missing {cid}");
        let mut corrupt = e.clone();
        corrupt.blocks.insert(cid.clone(), b"bad".to_vec());
        assert!(Backup::capture(&corrupt).is_err(), "corrupt {cid}");
    }
    let mut lost = e.clone();
    lost.blobs.clear();
    assert!(Backup::capture(&lost).is_err());
    let mut erased = e.clone();
    erased
        .private_archive_erasure_cids
        .insert(e.stored[0].cid.clone());
    assert!(Backup::capture(&erased).is_err());
}

#[test]
fn scrub_and_repair_missing_corrupt_files_and_reject_untrusted_source() {
    let (e, did) = fixture();
    let backup = Backup::capture(&e).unwrap();
    let dir = Dir::new();
    backup.restore(&dir.0, &did).unwrap();
    fs::remove_file(dir.0.join("blocks").join(&e.heads[&did].commit)).unwrap();
    fs::write(dir.0.join("heads.json"), b"broken").unwrap();
    let report = backup.scrub(&dir.0).unwrap();
    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.corrupt.len(), 1);
    let repaired = backup.repair(&dir.0).unwrap();
    assert_eq!(repaired.repaired_files, 2);
    assert_eq!(repaired.source_files_verified, repaired.expected_files);
    assert_eq!(
        repaired.source_payload_bytes_verified,
        repaired.expected_bytes
    );
    assert_eq!(repaired.post_repair_files_read, repaired.expected_files);
    assert_eq!(repaired.post_repair_bytes_read, repaired.expected_bytes);
    assert!(backup.scrub(&dir.0).unwrap().is_clean());
    assert_eq!(KnotEngine::open(&dir.0, &did).unwrap().blocks, e.blocks);
    let mut bytes = backup.to_bytes().unwrap();
    let n = bytes.len() / 2;
    bytes[n] ^= 1;
    assert!(Backup::from_bytes(&bytes).is_err());
    fs::write(dir.0.join("unknown"), b"keep").unwrap();
    assert!(backup.repair(&dir.0).is_err());
    assert_eq!(fs::read(dir.0.join("unknown")).unwrap(), b"keep");
}

#[test]
fn repair_respects_durable_erasure_fences() {
    let (e, did) = fixture();
    let backup = Backup::capture(&e).unwrap();
    let dir = Dir::new();
    backup.restore(&dir.0, &did).unwrap();
    fs::write(
        dir.0.join("private-archive-erasures.json"),
        serde_json::to_vec(&vec![(
            did.clone(),
            vec![e.stored[0].cid.clone()],
            Vec::<String>::new(),
        )])
        .unwrap(),
    )
    .unwrap();
    assert!(backup.repair(&dir.0).is_err());
}

#[test]
fn recovery_measurement() {
    let (mut e, did) = fixture();
    let response = e
        .handle(&KnotRequest {
            method: Method::Get,
            path: "/xrpc/tangled.sync.getRepo".into(),
            query: vec![("did".into(), did.clone()), ("blobs".into(), "true".into())],
            headers: vec![],
            body: None,
            content_type: None,
        })
        .unwrap();
    assert_eq!(response.status, 200);
    let car = tangled::car::decode(&response.body).unwrap();
    let cids: BTreeSet<_> = car.blocks.iter().map(|(c, _)| c.to_string()).collect();
    let baseline_records = e.stored.iter().filter(|r| cids.contains(&r.cid)).count();
    let baseline_dir = Dir::new();
    let mut baseline_engine = KnotEngine::open(&baseline_dir.0, &did).unwrap();
    baseline_engine.sessions.insert(
        "recovery-measurement".into(),
        tangled::session::SessionRecord {
            did: did.clone(),
            exp: baseline_engine.now + 60,
        },
    );
    let imported = baseline_engine
        .handle(&KnotRequest {
            method: Method::Post,
            path: "/xrpc/tangled.repo.import".into(),
            query: vec![("force".into(), "true".into())],
            headers: vec![(
                "Cookie".into(),
                format!("{}=recovery-measurement", tangled::session::COOKIE_NAME),
            )],
            body: Some(response.body.clone()),
            content_type: Some("application/vnd.ipld.car".into()),
        })
        .unwrap();
    assert_eq!(
        imported.status,
        200,
        "{}",
        String::from_utf8_lossy(&imported.body)
    );
    let baseline_reopened = KnotEngine::open(&baseline_dir.0, &did).unwrap();
    assert_eq!(baseline_reopened.stored.len(), 11);
    assert!(Backup::capture(&baseline_reopened).is_err());

    let source_dir = Dir::new();
    let mut persistent_source = e.clone();
    persistent_source.data_dir = Some(source_dir.0.clone());
    persistent_source.persist().unwrap();
    let copied_dir = Dir::new();
    let (copied_files, copied_bytes) = copy_tree(&source_dir.0, &copied_dir.0);
    let copied = KnotEngine::open(&copied_dir.0, &did).unwrap();
    assert_eq!(copied.blocks, e.blocks);
    assert_eq!(copied.stored.len(), 12);
    assert_eq!(copied.blobs, e.blobs);
    fs::write(
        copied_dir.0.join("blocks").join(&e.heads[&did].commit),
        b"bitrot",
    )
    .unwrap();
    let corrupt_copy = KnotEngine::open(&copied_dir.0, &did).unwrap();
    assert_ne!(
        corrupt_copy.blocks[&e.heads[&did].commit],
        e.blocks[&e.heads[&did].commit]
    );
    assert!(Backup::capture(&corrupt_copy).is_err());
    let backup = Backup::capture(&e).unwrap();
    let bytes = backup.to_bytes().unwrap();
    let dir = Dir::new();
    let restored = backup.restore(&dir.0, &did).unwrap();
    let clean = backup.scrub(&dir.0).unwrap();
    fs::remove_file(dir.0.join("blocks").join(&e.heads[&did].commit)).unwrap();
    fs::write(dir.0.join("heads.json"), b"broken").unwrap();
    let damaged = backup.scrub(&dir.0).unwrap();
    let repair = backup.repair(&dir.0).unwrap();
    let final_scrub = backup.scrub(&dir.0).unwrap();
    assert!(final_scrub.is_clean());
    let metrics = serde_json::json!({"fixture":{"records":12,"blobs":1,"blocks":e.blocks.len()},"file_copy_baseline":{"copied_files":copied_files,"copied_bytes":copied_bytes,"empty_store_restored_record_versions":copied.stored.len(),"empty_store_restored_blocks":copied.blocks.len(),"empty_store_restored_blobs":copied.blobs.len(),"corrupt_block_reopen_succeeds_without_detection":true,"corruption_rejected_by_treatment_capture":true},"public_sync_export":{"public_car_bytes":response.body.len(),"car_blocks":car.blocks.len(),"recoverable_record_versions":baseline_records,"import_status":imported.status,"empty_store_restored_record_versions":baseline_reopened.stored.len(),"empty_store_restored_blobs":baseline_reopened.blobs.len(),"full_dependency_closure_verified":false,"missing_source_blocks":e.blocks.keys().filter(|c|!cids.contains(*c)).count()},"treatment":{"full_dependency_closure_verified":true,"archive_bytes":bytes.len(),"restored_record_versions":restored.stored.len(),"restored_blobs":restored.blobs.len(),"restored_blocks":restored.blocks.len(),"scrub":clean,"damaged_scrub":damaged,"repair":repair,"post_repair_scrub":final_scrub}});
    println!(
        "RECOVERY_METRICS={}",
        serde_json::to_string_pretty(&metrics).unwrap()
    );
    if let Ok(path) = std::env::var("TANGLED_RECOVERY_METRICS") {
        fs::write(path, serde_json::to_vec_pretty(&metrics).unwrap()).unwrap();
    }
}

#[test]
fn restored_erasure_fences_survive_restart_without_extra_state() {
    let (mut e, did) = fixture();
    let erased = Cid::dag_cbor(b"already-erased-payload").to_string();
    e.private_archive_erasure_cids.insert(erased.clone());
    let backup = Backup::capture(&e).unwrap();
    let dir = Dir::new();
    let restored = backup.restore(&dir.0, &did).unwrap();
    assert!(restored.private_archive_erasure_cids.contains(&erased));
    assert!(backup.scrub(&dir.0).unwrap().is_clean());
    assert!(KnotEngine::open(&dir.0, &did)
        .unwrap()
        .private_archive_erasure_cids
        .contains(&erased));
}

#[test]
fn capture_rejects_tip_loss_even_when_content_blocks_remain() {
    let (mut e, _) = fixture();
    e.tips.clear();
    e.msts.clear();
    assert!(Backup::capture(&e).is_err());
}

#[test]
fn offline_cli_backup_verify_restore_scrub_and_repair() {
    use std::process::Command;
    let (mut e, did) = fixture();
    let source = Dir::new();
    e.data_dir = Some(source.0.clone());
    e.persist().unwrap();
    let archives = Dir::new();
    let archive = archives.0.join("backup.json");
    let target = Dir::new();
    let binary = env!("CARGO_BIN_EXE_tangled-recovery");
    let output = Command::new(binary)
        .args([
            "backup",
            source.0.to_str().unwrap(),
            &did,
            archive.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(binary)
        .args(["verify", archive.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new(binary)
        .args([
            "restore",
            archive.to_str().unwrap(),
            target.0.to_str().unwrap(),
            &did
        ])
        .output()
        .unwrap()
        .status
        .success());
    fs::write(target.0.join("heads.json"), b"bitrot").unwrap();
    assert_eq!(
        Command::new(binary)
            .args([
                "scrub",
                archive.to_str().unwrap(),
                target.0.to_str().unwrap()
            ])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    assert!(Command::new(binary)
        .args([
            "repair",
            archive.to_str().unwrap(),
            target.0.to_str().unwrap()
        ])
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new(binary)
        .args([
            "scrub",
            archive.to_str().unwrap(),
            target.0.to_str().unwrap()
        ])
        .output()
        .unwrap()
        .status
        .success());
    assert_eq!(KnotEngine::open(&target.0, &did).unwrap().stored.len(), 12);
    assert_eq!(
        Command::new(binary)
            .args([
                "restore",
                archive.to_str().unwrap(),
                target.0.to_str().unwrap(),
                &did
            ])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

#[cfg(unix)]
#[test]
fn restored_private_data_files_use_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (e, did) = fixture();
    let backup = Backup::capture(&e).unwrap();
    let parent = Dir::new();
    let target = parent.0.join("new-store");
    backup.restore(&target, &did).unwrap();
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(target.join("heads.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn repair_refuses_to_roll_back_a_valid_advanced_capability_counter() {
    let (e, did) = fixture();
    let backup = Backup::capture(&e).unwrap();
    let dir = Dir::new();
    backup.restore(&dir.0, &did).unwrap();
    let advanced = br#"{"already-used-capability":3}"#;
    fs::write(dir.0.join("cap-uses.json"), advanced).unwrap();
    assert!(backup.repair(&dir.0).is_err());
    assert_eq!(fs::read(dir.0.join("cap-uses.json")).unwrap(), advanced);
}
