use base64::{engine::general_purpose::STANDARD, Engine as _};
use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::{ArchiveStore, ImportBatch, ImportedObject};

fn sample(text: &str) -> ImportBatch {
    ImportBatch {
        source: "telegram".into(),
        account: "123".into(),
        objects: vec![ImportedObject {
            id: "chat:7/message:42".into(),
            kind: "message".into(),
            conversation: Some("chat:7".into()),
            sender: Some("user:5".into()),
            timestamp: Some("2024-01-02T03:04:05Z".into()),
            text: Some(text.into()),
            attachments: vec![],
            data: serde_json::json!({"id":42}),
        }],
        files: vec![],
        warnings: vec![],
    }
}

#[test]
fn reimports_are_private_durable_and_owner_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([31; 32], [32; 32]);
    let stranger = Identity::from_seeds([41; 32], [42; 32]);
    let mut knot = KnotEngine::hosting(&owner.did_string());
    knot.register_identity(&owner);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let first = store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut knot,
            "chat.json",
            b"original secret",
            &sample("a private message"),
        )
        .unwrap();
    let second = store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut knot,
            "chat.json",
            b"original secret",
            &sample("a private message"),
        )
        .unwrap();
    assert_eq!(first.inserted, 1);
    assert_eq!(second.inserted, 0);
    assert_eq!(store.objects(&owner.did_string(), &owner).unwrap().len(), 1);
    assert!(store
        .objects(&stranger.did_string(), &stranger)
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .original(&owner.did_string(), &owner, &first.id)
            .unwrap()
            .1,
        b"original secret"
    );
    assert!(store
        .original(&stranger.did_string(), &stranger, &first.id)
        .is_err());
    assert!(knot.stored.iter().all(|r| r.vis == "sealed"));
    assert!(knot
        .blobs
        .values()
        .all(|b| !b.windows(15).any(|w| w == b"original secret")));
    drop(store);
    let reopened = ArchiveStore::open(dir.path()).unwrap();
    assert_eq!(
        reopened.objects(&owner.did_string(), &owner).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("a private message")
    );
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes.windows(17).any(|w| w == b"a private message"));
    }
}

#[test]
fn disconnected_tokens_cannot_ingest_and_other_accounts_cannot_see_them() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let created = store.connect("did:sol:owner", "telegram", "123").unwrap();
    assert!(store
        .authorize_ingest(&created.connection.id, &created.token)
        .is_ok());
    assert!(store.connections("did:sol:other").unwrap().is_empty());
    assert!(store
        .disconnect("did:sol:other", &created.connection.id)
        .is_err());
    store
        .disconnect("did:sol:owner", &created.connection.id)
        .unwrap();
    assert!(store
        .authorize_ingest(&created.connection.id, &created.token)
        .is_err());
}

#[test]
fn portable_backup_rewraps_archive_keys_and_preserves_originals() {
    let old = Identity::from_seeds([51; 32], [52; 32]);
    let next = Identity::from_seeds([61; 32], [62; 32]);
    let mut knot = KnotEngine::hosting(&old.did_string());
    knot.register_identity(&old);
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let mut store = ArchiveStore::open(src.path()).unwrap();
    let receipt = store
        .ingest(
            &old.did_string(),
            &old,
            &mut knot,
            "history.json",
            b"history",
            &sample("keep this"),
        )
        .unwrap();
    let backup = store.backup(&old.did_string(), &old).unwrap();
    let mut destination = ArchiveStore::open(dst.path()).unwrap();
    destination
        .restore(&old.did_string(), &next, &backup)
        .unwrap();
    assert_eq!(
        destination.objects(&old.did_string(), &next).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("keep this")
    );
    assert_eq!(
        destination
            .original(&old.did_string(), &next, &receipt.id)
            .unwrap()
            .1,
        b"history"
    );
    assert!(destination
        .connections(&old.did_string())
        .unwrap()
        .is_empty());
}

#[test]
fn reused_export_filenames_do_not_replace_earlier_message_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([71; 32], [72; 32]);
    let did = owner.did_string();
    let mut knot = KnotEngine::hosting(&did);
    knot.register_identity(&owner);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut first = sample("first photo");
    first.objects[0].attachments = vec!["photo.jpg".into()];
    first.files = vec![tangled_import::ImportedFile {
        name: "photo.jpg".into(),
        media_type: "image/jpeg".into(),
        data_base64: STANDARD.encode(b"first image"),
    }];
    let mut second = sample("second photo");
    second.objects[0].id = "another-message".into();
    second.objects[0].attachments = vec!["photo.jpg".into()];
    second.files = vec![tangled_import::ImportedFile {
        name: "photo.jpg".into(),
        media_type: "image/jpeg".into(),
        data_base64: STANDARD.encode(b"second image"),
    }];
    store
        .ingest(&did, &owner, &mut knot, "first.zip", b"first", &first)
        .unwrap();
    store
        .ingest(&did, &owner, &mut knot, "second.zip", b"second", &second)
        .unwrap();
    let objects = store.objects(&did, &owner).unwrap();
    let first = objects
        .iter()
        .find(|o| o.object.text.as_deref() == Some("first photo"))
        .unwrap();
    let second = objects
        .iter()
        .find(|o| o.object.text.as_deref() == Some("second photo"))
        .unwrap();
    assert_ne!(first.object.attachments[0], second.object.attachments[0]);
    assert_eq!(
        store
            .attachment(
                &did,
                &owner,
                "telegram",
                "123",
                &first.object.attachments[0]
            )
            .unwrap()
            .1,
        b"first image"
    );
    assert_eq!(
        store
            .attachment(
                &did,
                &owner,
                "telegram",
                "123",
                &second.object.attachments[0]
            )
            .unwrap()
            .1,
        b"second image"
    );
}

#[test]
fn corrupt_backup_is_rejected_without_replacing_existing_archive_keys() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([81; 32], [82; 32]);
    let did = owner.did_string();
    let mut knot = KnotEngine::hosting(&did);
    knot.register_identity(&owner);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let receipt = store
        .ingest(
            &did,
            &owner,
            &mut knot,
            "old.json",
            b"original",
            &sample("untouched"),
        )
        .unwrap();
    let mut broken = store.backup(&did, &owner).unwrap();
    broken.chunks.clear();
    assert!(store.restore(&did, &owner, &broken).is_err());
    assert_eq!(
        store.objects(&did, &owner).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("untouched")
    );
    assert_eq!(
        store.original(&did, &owner, &receipt.id).unwrap().1,
        b"original"
    );
}

#[test]
fn restoring_a_different_archive_key_does_not_corrupt_existing_history() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([91; 32], [92; 32]);
    let did = owner.did_string();
    let mut ka = KnotEngine::hosting(&did);
    ka.register_identity(&owner);
    let mut kb = KnotEngine::hosting(&did);
    kb.register_identity(&owner);
    let mut sa = ArchiveStore::open(a.path()).unwrap();
    let mut sb = ArchiveStore::open(b.path()).unwrap();
    sa.ingest(&did, &owner, &mut ka, "a.json", b"a", &sample("from a"))
        .unwrap();
    sb.ingest(&did, &owner, &mut kb, "b.json", b"b", &sample("from b"))
        .unwrap();
    assert!(sb
        .restore(&did, &owner, &sa.backup(&did, &owner).unwrap())
        .is_err());
    assert_eq!(
        sb.objects(&did, &owner).unwrap()[0].object.text.as_deref(),
        Some("from b")
    );
}

#[test]
fn source_timestamps_sort_by_instants_across_archive_and_api_formats() {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([93; 32], [94; 32]);
    let did = owner.did_string();
    let mut knot = KnotEngine::hosting(&did);
    knot.register_identity(&owner);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let mut batch = sample("older");
    batch.source = "twitter".into();
    batch.objects[0].id = "old".into();
    batch.objects[0].timestamp = Some("Wed Jan 01 11:00:00 +0000 2020".into());
    let mut newer = batch.objects[0].clone();
    newer.id = "new".into();
    newer.text = Some("newer".into());
    newer.timestamp = Some("2024-01-01T10:00:00Z".into());
    batch.objects.push(newer);
    store
        .ingest(&did, &owner, &mut knot, "dates.json", b"dates", &batch)
        .unwrap();
    assert_eq!(
        store.objects(&did, &owner).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("newer")
    );
}
