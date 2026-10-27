use serde_json::json;
use tangled_personal::Store;

#[test]
fn signed_private_records_roundtrip_and_causal_update() {
    let mut store = Store::memory();
    let first = store
        .put(
            "pub.example.note",
            "one",
            json!({"text":"secret","extension":{"x":1}}),
            "cap",
        )
        .unwrap();
    assert_eq!(first["vis"], "cap");
    assert!(first["sig"].as_str().unwrap().len() == 128);
    assert!(first["uri"]
        .as_str()
        .unwrap()
        .starts_with("tangled://did:sol:"));
    let second = store
        .put("pub.example.note", "one", json!({"text":"new"}), "cap")
        .unwrap();
    assert_eq!(second["prev"][0], first["cid"]);
    assert_eq!(store.list(None).unwrap().len(), 1);
    assert_eq!(
        store.get("pub.example.note", "one").unwrap()["body"]["text"],
        "new"
    );
    store.remove("pub.example.note", "one").unwrap();
    assert!(store.list(None).unwrap().is_empty());
    assert!(store.get("pub.example.note", "one").is_err());
}

#[test]
fn persistent_owner_and_blob_bytes_survive_reopen_and_exclusive_lock() {
    let dir = tempfile::tempdir().unwrap();
    let did;
    let cid;
    {
        let mut store = Store::open(dir.path()).unwrap();
        did = store.did();
        assert!(Store::open(dir.path()).is_err());
        cid = store.put_blob(b"private attachment", "text/plain").unwrap();
        store
            .put("pub.example.file", "f", json!({"blob":cid}), "cap")
            .unwrap();
    }
    let mut reopened = Store::open(dir.path()).unwrap();
    assert_eq!(reopened.did(), did);
    assert_eq!(reopened.get_blob(&cid).unwrap(), b"private attachment");
    assert_eq!(reopened.list(None).unwrap().len(), 1);
}

#[test]
fn invalid_inputs_and_incompatible_foreign_backup_do_not_mutate_store() {
    let mut store = Store::memory();
    assert!(store
        .put("pub.example.note", "bad/key", json!({}), "cap")
        .is_err());
    assert!(store
        .put("pub.example.note", "one", json!({}), "sealed")
        .is_err());
    store
        .put("pub.example.note", "one", json!({"text":"keep"}), "cap")
        .unwrap();
    let mut foreign = Store::memory();
    foreign
        .put(
            "pub.example.note",
            "other",
            json!({"text":"foreign"}),
            "cap",
        )
        .unwrap();
    assert!(store.import(&foreign.export().unwrap()).is_err());
    assert_eq!(store.list(None).unwrap().len(), 1);
    assert_eq!(
        store.get("pub.example.note", "one").unwrap()["body"]["text"],
        "keep"
    );
}

#[test]
fn owner_can_read_attachment_before_linking_it_to_record() {
    let mut store = Store::memory();
    let cid = store.put_blob(b"unlinked", "text/plain").unwrap();
    assert_eq!(store.get_blob(&cid).unwrap(), b"unlinked");
}

#[test]
fn generic_protocol_client_reads_public_and_cannot_read_private_records() {
    use tangled::http::{KnotRequest, Method};
    let dir = tempfile::tempdir().unwrap();
    let did;
    {
        let mut store = Store::open(dir.path()).unwrap();
        did = store.did();
        store
            .put(
                "pub.example.alternativeApp",
                "private",
                json!({"text":"private","extensions":{"other":true}}),
                "cap",
            )
            .unwrap();
        store
            .put(
                "pub.example.alternativeApp",
                "public",
                json!({"text":"public"}),
                "public",
            )
            .unwrap();
    }
    let mut client = tangled::KnotEngine::open(dir.path(), &did).unwrap();
    for (key, status) in [("private", 401), ("public", 200)] {
        let result = client
            .handle(&KnotRequest {
                method: Method::Get,
                path: "/xrpc/tangled.sync.getRecord".into(),
                query: vec![
                    ("did".into(), did.clone()),
                    ("collection".into(), "pub.example.alternativeApp".into()),
                    ("rkey".into(), key.into()),
                ],
                headers: vec![],
                body: None,
                content_type: None,
            })
            .unwrap();
        assert_eq!(result.status, status);
    }
}

fn replica(source: &std::path::Path, destination: &std::path::Path) -> Store {
    std::fs::write(
        destination.join("identity.json"),
        std::fs::read(source.join("identity.json")).unwrap(),
    )
    .unwrap();
    Store::open(destination).unwrap()
}

#[test]
fn import_uses_signed_mst_tips_independent_of_car_order() {
    let source_dir = tempfile::tempdir().unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let mut source = Store::open(source_dir.path()).unwrap();
    source
        .put("pub.tangled.note", "one", json!({"text":"old"}), "cap")
        .unwrap();
    let latest = source
        .put(
            "pub.tangled.note",
            "one",
            json!({"text":"new","extension":{"keep":true}}),
            "cap",
        )
        .unwrap();
    let mut car = tangled::car::decode(&source.export().unwrap()).unwrap();
    car.blocks.reverse();
    let mut target = replica(source_dir.path(), target_dir.path());
    target.import(&tangled::car::encode(&car).unwrap()).unwrap();
    assert_eq!(
        target.get("pub.tangled.note", "one").unwrap()["cid"],
        latest["cid"]
    );
    drop(target);
    let mut target = Store::open(target_dir.path()).unwrap();
    assert_eq!(
        target.get("pub.tangled.note", "one").unwrap()["body"],
        latest["body"]
    );
}

#[test]
fn import_rejects_missing_signed_mst_and_record_closures_before_changing_head() {
    let source_dir = tempfile::tempdir().unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let mut source = Store::open(source_dir.path()).unwrap();
    let first = source
        .put("pub.tangled.note", "one", json!({"text":"old"}), "cap")
        .unwrap();
    source
        .put("pub.tangled.note", "one", json!({"text":"new"}), "cap")
        .unwrap();
    let car = tangled::car::decode(&source.export().unwrap()).unwrap();
    let mut target = replica(source_dir.path(), target_dir.path());
    let mut incomplete = car.clone();
    incomplete
        .blocks
        .retain(|(_, bytes)| tangled::record::parse_commit(bytes).is_ok());
    assert!(target
        .import(&tangled::car::encode(&incomplete).unwrap())
        .is_err());
    incomplete = car.clone();
    incomplete
        .blocks
        .retain(|(cid, _)| cid.to_string() != first["cid"].as_str().unwrap());
    assert!(
        target
            .import(&tangled::car::encode(&incomplete).unwrap())
            .is_err(),
        "historical signed MST record is required"
    );
    assert!(target.list(None).unwrap().is_empty());
    target.import(&tangled::car::encode(&car).unwrap()).unwrap();
    assert_eq!(
        target.get("pub.tangled.note", "one").unwrap()["body"]["text"],
        "new"
    );
}

#[test]
fn import_filesystem_failure_preserves_durable_head_and_content() {
    let source_dir = tempfile::tempdir().unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let mut source = Store::open(source_dir.path()).unwrap();
    let mut target = replica(source_dir.path(), target_dir.path());
    source
        .put("pub.tangled.note", "one", json!({"text":"old"}), "cap")
        .unwrap();
    target.import(&source.export().unwrap()).unwrap();
    let before = std::fs::read(target_dir.path().join("heads.json")).unwrap();
    let new = source
        .put("pub.tangled.note", "one", json!({"text":"new"}), "cap")
        .unwrap();
    let obstruction = target_dir
        .path()
        .join("repos")
        .join(source.did().replace(['/', '\\', ':'], "_"))
        .join("collections/pub.tangled.note/one")
        .join(format!("{}.json", new["cid"].as_str().unwrap()));
    std::fs::create_dir(&obstruction).unwrap();
    assert!(target.import(&source.export().unwrap()).is_err());
    assert_eq!(
        std::fs::read(target_dir.path().join("heads.json")).unwrap(),
        before
    );
    assert_eq!(
        target.get("pub.tangled.note", "one").unwrap()["body"]["text"],
        "old"
    );
    drop(target);
    std::fs::remove_dir(obstruction).unwrap();
    let mut target = Store::open(target_dir.path()).unwrap();
    assert_eq!(
        target.get("pub.tangled.note", "one").unwrap()["body"]["text"],
        "old"
    );
    target.import(&source.export().unwrap()).unwrap();
    assert_eq!(
        target.get("pub.tangled.note", "one").unwrap()["body"]["text"],
        "new"
    );
}

#[test]
fn import_keeps_uncommitted_signed_record_blocks_out_of_current_records() {
    let source_dir = tempfile::tempdir().unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let mut source = Store::open(source_dir.path()).unwrap();
    let current = source
        .put(
            "pub.tangled.note",
            "one",
            json!({"text":"committed"}),
            "cap",
        )
        .unwrap();
    let identity_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source_dir.path().join("identity.json")).unwrap())
            .unwrap();
    let identity = tangled::Identity::from_seeds(
        hex::decode(identity_json["ed25519"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
        hex::decode(identity_json["x25519"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let mut unsigned = current.clone();
    for key in ["cid", "uri", "sig"] {
        unsigned.as_object_mut().unwrap().remove(key);
    }
    unsigned["rkey"] = json!("uncommitted");
    let rogue = tangled::record::sign_json_record(identity.signing_key(), &unsigned).unwrap();
    let mut car = tangled::car::decode(&source.export().unwrap()).unwrap();
    car.blocks.push((rogue.cid, rogue.signed_cbor));
    let mut target = replica(source_dir.path(), target_dir.path());
    target.import(&tangled::car::encode(&car).unwrap()).unwrap();
    assert_eq!(target.list(None).unwrap().len(), 1);
    assert!(target.get("pub.tangled.note", "uncommitted").is_err());
}

#[test]
fn persistent_writes_reject_escaped_key_collisions_before_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let first = store
        .put(
            "pub.tangled.note",
            "a:b",
            json!({"text":"keep colon key"}),
            "cap",
        )
        .unwrap();
    let heads = std::fs::read(dir.path().join("heads.json")).unwrap();
    assert!(store
        .put(
            "pub.tangled.note",
            "a_b",
            json!({"text":"collision"}),
            "cap"
        )
        .is_err());
    assert_eq!(std::fs::read(dir.path().join("heads.json")).unwrap(), heads);
    assert_eq!(
        store.get("pub.tangled.note", "a:b").unwrap()["cid"],
        first["cid"]
    );
    assert!(store.get("pub.tangled.note", "a_b").is_err());
    drop(store);
    let mut reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        reopened.get("pub.tangled.note", "a:b").unwrap()["cid"],
        first["cid"]
    );
    assert!(reopened.get("pub.tangled.note", "a_b").is_err());
    reopened.remove("pub.tangled.note", "a:b").unwrap();
    assert!(reopened
        .put(
            "pub.tangled.note",
            "a_b",
            json!({"text":"collision with tombstone"}),
            "cap"
        )
        .is_err());
}
