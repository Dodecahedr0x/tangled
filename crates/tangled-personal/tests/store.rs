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
