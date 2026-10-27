use serde_json::{json, Value};
use tangled_personal::{devices::execute, Store};

const PASS: &str = "an independent recovery passphrase";
fn backup(store: &mut Store, file: &std::path::Path) -> Value {
    execute(
        store,
        json!({"action":"backup","output_path":file,"passphrase":PASS}),
    )
    .unwrap()
}

#[test]
fn portable_restore_preserves_owner_records_and_all_attachment_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = Store::memory();
    let linked = source.put_blob(b"linked attachment", "text/plain").unwrap();
    let unlinked = source
        .put_blob(b"unlinked attachment", "text/plain")
        .unwrap();
    source
        .put(
            "pub.tangled.note",
            "one",
            json!({"title":"private note","blob":linked}),
            "cap",
        )
        .unwrap();
    let file = dir.path().join("portable.json");
    let result = backup(&mut source, &file);
    assert_eq!(result["owner"], source.did());
    let bytes = std::fs::read(&file).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains(PASS));
    assert!(!text.contains("private note"));
    assert!(!text.contains("ed25519"));
    let verified = execute(
        &mut source,
        json!({"action":"verify","input_path":file,"passphrase":PASS}),
    )
    .unwrap();
    assert_eq!(verified["records"], 1);
    assert!(verified["blobs"].as_u64().unwrap() >= 3);
    let target = dir.path().join("restored");
    execute(
        &mut source,
        json!({"action":"restore","input_path":file,"destination":target,"passphrase":PASS}),
    )
    .unwrap();
    let mut restored = Store::open(&target).unwrap();
    assert_eq!(restored.did(), source.did());
    assert_eq!(
        restored.get("pub.tangled.note", "one").unwrap()["body"]["title"],
        "private note"
    );
    assert_eq!(restored.get_blob(&linked).unwrap(), b"linked attachment");
    assert_eq!(
        restored.get_blob(&unlinked).unwrap(),
        b"unlinked attachment"
    );
    restored
        .put(
            "pub.tangled.note",
            "two",
            json!({"title":"continued"}),
            "cap",
        )
        .unwrap();
    drop(restored);
    assert_eq!(Store::open(&target).unwrap().list(None).unwrap().len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(target.join("identity.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn backup_authentication_and_no_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::memory();
    store
        .put("pub.tangled.note", "one", json!({"text":"secret"}), "cap")
        .unwrap();
    let path = dir.path().join("backup.json");
    backup(&mut store, &path);
    assert!(execute(
        &mut store,
        json!({"action":"backup","output_path":path,"passphrase":PASS})
    )
    .is_err());
    assert!(execute(
        &mut store,
        json!({"action":"verify","input_path":path,"passphrase":"wrong but sufficiently long"})
    )
    .is_err());
    let mut packet: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let ct = packet["ct"].as_str().unwrap().to_owned();
    packet["ct"] = json!(format!(
        "{}{}",
        if &ct[..2] == "00" { "01" } else { "00" },
        &ct[2..]
    ));
    std::fs::write(&path, serde_json::to_vec(&packet).unwrap()).unwrap();
    let target = dir.path().join("tampered-target");
    assert!(execute(
        &mut store,
        json!({"action":"restore","input_path":path,"destination":target,"passphrase":PASS})
    )
    .is_err());
    assert!(!target.exists());
}

#[test]
fn transfer_accepts_successor_rejects_stale_forks_and_other_identity() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = Store::memory();
    source
        .put("pub.tangled.note", "one", json!({"n":1}), "cap")
        .unwrap();
    let original = dir.path().join("original.json");
    backup(&mut source, &original);
    let target = dir.path().join("replica");
    execute(
        &mut source,
        json!({"action":"restore","input_path":original,"destination":target,"passphrase":PASS}),
    )
    .unwrap();
    assert!(execute(
        &mut source,
        json!({"action":"restore","input_path":original,"destination":target,"passphrase":PASS})
    )
    .is_err());
    let mut replica = Store::open(&target).unwrap();
    execute(
        &mut replica,
        json!({"action":"sync","input_path":original,"passphrase":PASS}),
    )
    .unwrap();
    source
        .put("pub.tangled.note", "one", json!({"n":2}), "cap")
        .unwrap();
    let newer = dir.path().join("newer.json");
    backup(&mut source, &newer);
    execute(
        &mut replica,
        json!({"action":"sync","input_path":newer,"passphrase":PASS}),
    )
    .unwrap();
    assert_eq!(
        replica.get("pub.tangled.note", "one").unwrap()["body"]["n"],
        2
    );
    assert!(execute(
        &mut replica,
        json!({"action":"sync","input_path":original,"passphrase":PASS})
    )
    .is_err());
    source
        .put(
            "pub.tangled.note",
            "branch",
            json!({"side":"source"}),
            "cap",
        )
        .unwrap();
    replica
        .put(
            "pub.tangled.note",
            "branch",
            json!({"side":"replica"}),
            "cap",
        )
        .unwrap();
    let fork = dir.path().join("fork.json");
    backup(&mut source, &fork);
    assert!(execute(
        &mut replica,
        json!({"action":"sync","input_path":fork,"passphrase":PASS})
    )
    .is_err());
    let mut stranger = Store::memory();
    assert!(execute(
        &mut stranger,
        json!({"action":"sync","input_path":newer,"passphrase":PASS})
    )
    .is_err());
    assert_eq!(
        replica.get("pub.tangled.note", "branch").unwrap()["body"]["side"],
        "replica"
    );
}

#[test]
fn empty_owner_is_recoverable_and_inputs_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::memory();
    let path = dir.path().join("empty.json");
    assert!(execute(
        &mut store,
        json!({"action":"backup","output_path":path,"passphrase":"short"})
    )
    .is_err());
    assert!(!path.exists());
    backup(&mut store, &path);
    let target = dir.path().join("owner");
    execute(
        &mut store,
        json!({"action":"restore","input_path":path,"destination":target,"passphrase":PASS}),
    )
    .unwrap();
    assert_eq!(Store::open(target).unwrap().did(), store.did());
    assert!(execute(&mut store, json!({"action":"pair"})).is_err());
}

#[test]
fn rejects_oversized_file_and_changed_authenticated_owner() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::memory();
    let path = dir.path().join("oversized.json");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(128 * 1024 * 1024 + 4097).unwrap();
    let result = execute(
        &mut store,
        json!({"action":"verify","input_path":path,"passphrase":PASS}),
    );
    assert!(result.unwrap_err().contains("size limit"));
    std::fs::remove_file(&path).unwrap();
    backup(&mut store, &path);
    let mut packet: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    packet["owner"] = json!(Store::memory().did());
    std::fs::write(&path, serde_json::to_vec(&packet).unwrap()).unwrap();
    assert!(execute(
        &mut store,
        json!({"action":"verify","input_path":path,"passphrase":PASS})
    )
    .unwrap_err()
    .contains("damaged backup"));
}
