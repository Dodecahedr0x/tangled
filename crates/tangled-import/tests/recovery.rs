use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::{recovery_export, recovery_open, ArchiveStore, ImportBatch};

#[test]
fn owner_can_recover_identity_and_archive_without_the_original_provider() {
    let dir = tempfile::tempdir().unwrap();
    let id = Identity::from_seeds([101; 32], [102; 32]);
    let did = id.did_string();
    let mut knot = KnotEngine::hosting(&did);
    knot.register_identity(&id);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut batch:ImportBatch=serde_json::from_value(serde_json::json!({"source":"telegram","account":"123","objects":[{"id":"1","kind":"message","text":"portable private message"}]})).unwrap();
    batch.files.push(tangled_import::ImportedFile {
        name: "photo.jpg".into(),
        media_type: "image/jpeg".into(),
        data_base64: "cmVzdG9yZWQgbWVkaWE=".into(),
    });
    batch.objects[0].attachments.push("photo.jpg".into());
    let receipt = store
        .ingest(
            &did,
            &id,
            &mut knot,
            "history.json",
            b"exact history",
            &batch,
        )
        .unwrap();
    let packet = recovery_export(
        &did,
        &id,
        &store.backup(&did, &id).unwrap(),
        "a separate recovery passphrase",
    )
    .unwrap();
    let serialized = serde_json::to_string(&packet).unwrap();
    assert!(!serialized.contains("portable private message"));
    assert!(!serialized.contains(&hex::encode([101u8; 32])));
    assert!(recovery_open(&packet, "wrong passphrase").is_err());
    let recovered = recovery_open(&packet, "a separate recovery passphrase").unwrap();
    let identity = recovered.identity().unwrap();
    assert_eq!(identity.to_seeds(), id.to_seeds());
    let next = tempfile::tempdir().unwrap();
    let mut restored = ArchiveStore::open(next.path()).unwrap();
    restored
        .restore(&did, &identity, &recovered.archive)
        .unwrap();
    assert_eq!(
        restored.objects(&did, &identity).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("portable private message")
    );
    assert_eq!(
        restored.original(&did, &identity, &receipt.id).unwrap().1,
        b"exact history"
    );
    let mut tampered = packet.clone();
    tampered["owner"] = serde_json::json!("another owner");
    assert!(recovery_open(&tampered, "a separate recovery passphrase").is_err());
    let input = tempfile::tempdir().unwrap();
    let backup_path = input.path().join("backup.json");
    let passphrase_path = input.path().join("passphrase");
    let output_path = input.path().join("recovered");
    std::fs::write(&backup_path, serde_json::to_vec(&packet).unwrap()).unwrap();
    std::fs::write(&passphrase_path, "a separate recovery passphrase\n").unwrap();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_tangled-import-recover"))
        .args([
            backup_path.as_os_str(),
            passphrase_path.as_os_str(),
            output_path.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let activity = std::fs::read_to_string(output_path.join("activity.json")).unwrap();
    assert!(activity.contains("portable private message"));
    assert_eq!(
        std::fs::read(output_path.join(format!("original-{}", receipt.id))).unwrap(),
        b"exact history"
    );
    let keys: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_path.join("recovered-account.json")).unwrap())
            .unwrap();
    assert_eq!(keys["ed25519"], hex::encode([101u8; 32]));
    let files: Vec<tangled_import::ArchiveFileMetadata> =
        serde_json::from_slice(&std::fs::read(output_path.join("files.json")).unwrap()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        std::fs::read(output_path.join("attachments").join(&files[0].key)).unwrap(),
        b"restored media"
    );
}
