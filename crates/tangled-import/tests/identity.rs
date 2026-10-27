use serde_json::json;
use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::{parse, ArchiveStore, ImportBatch, ImportedObject};

fn setup() -> (tempfile::TempDir, Identity, KnotEngine, ArchiveStore) {
    let dir = tempfile::tempdir().unwrap();
    let owner = Identity::from_seeds([71; 32], [72; 32]);
    let mut engine = KnotEngine::hosting(&owner.did_string());
    engine.register_identity(&owner);
    let store = ArchiveStore::open(dir.path()).unwrap();
    (dir, owner, engine, store)
}

fn batch(source: &str, object: ImportedObject) -> ImportBatch {
    ImportBatch {
        source: source.into(),
        account: "42".into(),
        objects: vec![object],
        files: vec![],
        warnings: vec![],
    }
}

fn message(
    id: &str,
    conversation: Option<&str>,
    text: &str,
    data: serde_json::Value,
) -> ImportedObject {
    ImportedObject {
        id: id.into(),
        kind: "message".into(),
        conversation: conversation.map(str::to_owned),
        sender: Some("99".into()),
        timestamp: None,
        text: Some(text.into()),
        attachments: vec![],
        data,
    }
}

#[test]
fn x_native_post_is_one_object_across_archive_live_and_conversation_enrichment() {
    let (_dir, owner, mut engine, mut store) = setup();
    let archived = parse("twitter", "42", "tweets.js",
        br#"window.YTD.tweets.part0 = [{"tweet":{"id_str":"100","full_text":"hello","created_at":"Tue Oct 27 12:00:00 +0000 2026"}}];"#).unwrap();
    store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "tweets.js",
            b"archive",
            &archived,
        )
        .unwrap();
    let mut live = archived.clone();
    live.objects[0].conversation = Some("90".into());
    live.objects[0].data = json!({"post":{"id":"100","conversation_id":"90","text":"hello","created_at":"2026-10-27T12:00:00Z"}});
    store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "live.json",
            b"live",
            &live,
        )
        .unwrap();
    let objects = store.objects(&owner.did_string(), &owner).unwrap();
    assert_eq!(
        objects.len(),
        1,
        "a native X post must not duplicate when conversation metadata appears"
    );
    store
        .delete_object(&owner.did_string(), &objects[0].key)
        .unwrap();
    live.objects[0].conversation = Some("different-enrichment".into());
    store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "replay.json",
            b"replay",
            &live,
        )
        .unwrap();
    assert!(store
        .objects(&owner.did_string(), &owner)
        .unwrap()
        .is_empty());
}

#[test]
fn x_native_dm_and_relationship_ids_ignore_optional_conversation_metadata() {
    let (_dir, owner, mut engine, mut store) = setup();
    for (kind, id) in [
        ("message", "200"),
        ("like", "like:101"),
        ("bookmark", "bookmark:102"),
        ("following", "following:99"),
        ("follower", "follower:98"),
    ] {
        let mut item = message(id, None, "same", json!({"id":id}));
        item.kind = kind.into();
        let first = batch("twitter", item);
        store
            .ingest(
                &owner.did_string(),
                &owner,
                &mut engine,
                &format!("{kind}-archive"),
                kind.as_bytes(),
                &first,
            )
            .unwrap();
        let mut second = first.clone();
        second.objects[0].conversation = Some("42-99".into());
        store
            .ingest(
                &owner.did_string(),
                &owner,
                &mut engine,
                &format!("{kind}-live"),
                b"live",
                &second,
            )
            .unwrap();
    }
    assert_eq!(store.objects(&owner.did_string(), &owner).unwrap().len(), 5);
}

#[test]
fn telegram_archive_peer_types_match_live_marked_ids_without_colliding() {
    let (_dir, owner, mut engine, mut store) = setup();
    for (kind, marked) in [
        ("personal_chat", "123"),
        ("private_group", "-123"),
        ("private_supergroup", "-1000000000123"),
    ] {
        let raw = serde_json::to_vec(
            &json!({"id":123, "type":kind, "messages":[{"id":1,"type":"message","text":"same"}]}),
        )
        .unwrap();
        let archived = parse("telegram", "42", "result.json", &raw).unwrap();
        let msg = archived
            .objects
            .iter()
            .find(|o| o.kind == "message")
            .unwrap();
        assert_eq!(msg.id, format!("{marked}:1"));
        assert_eq!(msg.conversation.as_deref(), Some(marked));
        store
            .ingest(
                &owner.did_string(),
                &owner,
                &mut engine,
                &format!("{kind}.json"),
                &raw,
                &archived,
            )
            .unwrap();
        let live = batch(
            "telegram",
            message(
                &format!("{marked}:1"),
                Some(marked),
                "same",
                json!({"id":1}),
            ),
        );
        store
            .ingest(
                &owner.did_string(),
                &owner,
                &mut engine,
                &format!("{kind}-live"),
                b"live",
                &live,
            )
            .unwrap();
    }
    let messages = store
        .objects(&owner.did_string(), &owner)
        .unwrap()
        .into_iter()
        .filter(|o| o.object.kind == "message")
        .count();
    assert_eq!(messages, 3);
}

#[test]
fn telegram_archive_then_newer_live_then_older_archive_preserves_newest_edit() {
    let (_dir, owner, mut engine, mut store) = setup();
    let archive = batch(
        "telegram",
        message(
            "123:1",
            Some("123"),
            "old",
            json!({"edited_unixtime":"100"}),
        ),
    );
    store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "archive.json",
            b"old",
            &archive,
        )
        .unwrap();
    let live = batch(
        "telegram",
        message(
            "123:1",
            Some("123"),
            "new",
            json!({"edit_date":"1970-01-01T00:03:20Z"}),
        ),
    );
    let receipt = store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "live.json",
            b"new",
            &live,
        )
        .unwrap();
    assert_eq!(receipt.updated, 1);
    assert_eq!(
        store.objects(&owner.did_string(), &owner).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("new")
    );
    let stale = store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "older-again.json",
            b"older-again",
            &archive,
        )
        .unwrap();
    assert_eq!(stale.unchanged, 1);
    assert_eq!(
        store.objects(&owner.did_string(), &owner).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("new")
    );
    assert_eq!(
        store
            .original(&owner.did_string(), &owner, &stale.id)
            .unwrap()
            .1,
        b"older-again"
    );
}

#[test]
fn unordered_conflict_keeps_existing_object_warns_and_retains_incoming_raw() {
    let (_dir, owner, mut engine, mut store) = setup();
    let first = batch(
        "telegram",
        message("123:1", Some("123"), "existing", json!({})),
    );
    store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "first.json",
            b"first",
            &first,
        )
        .unwrap();
    let unknown = batch(
        "telegram",
        message("123:1", Some("123"), "uncertain", json!({})),
    );
    let receipt = store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "unknown.json",
            b"unorderable source bytes",
            &unknown,
        )
        .unwrap();
    assert_eq!(receipt.unchanged, 1);
    assert!(receipt.warnings.iter().any(|w| w.contains("revision")));
    assert_eq!(
        store.objects(&owner.did_string(), &owner).unwrap()[0]
            .object
            .text
            .as_deref(),
        Some("existing")
    );
    assert_eq!(
        store
            .original(&owner.did_string(), &owner, &receipt.id)
            .unwrap()
            .1,
        b"unorderable source bytes"
    );
}

#[test]
fn ambiguous_telegram_peers_and_whatsapp_text_do_not_claim_live_deduplication() {
    let telegram = parse(
        "telegram",
        "42",
        "result.json",
        br#"{"id":123,"messages":[{"id":1,"text":"hello"}]}"#,
    )
    .unwrap();
    assert!(telegram
        .warnings
        .iter()
        .any(|w| w.contains("peer") && w.contains("overlap")));
    let whatsapp = parse(
        "whatsapp",
        "42",
        "chat.txt",
        b"[27/10/2026, 09:30] Bob: hello",
    )
    .unwrap();
    assert!(whatsapp.warnings.iter().any(|w| w.contains("overlap")));
}

#[test]
fn same_source_payload_can_resolve_previously_missing_media() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use tangled_import::ImportedFile;
    let (_dir, owner, mut engine, mut store) = setup();
    let mut item = message(
        "123:1",
        Some("123"),
        "photo",
        json!({"date_unixtime":"100","photo":"pic.jpg"}),
    );
    item.attachments = vec!["pic.jpg".into()];
    let first = batch("telegram", item);
    store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "first.json",
            b"json without media",
            &first,
        )
        .unwrap();
    let mut with_media = first.clone();
    with_media.files.push(ImportedFile {
        name: "pic.jpg".into(),
        media_type: "image/jpeg".into(),
        data_base64: STANDARD.encode(b"photo bytes"),
    });
    let receipt = store
        .ingest(
            &owner.did_string(),
            &owner,
            &mut engine,
            "complete.zip",
            b"zip with media",
            &with_media,
        )
        .unwrap();
    assert_eq!(receipt.updated, 1);
    let stored = store.objects(&owner.did_string(), &owner).unwrap();
    let reference = &stored[0].object.attachments[0];
    assert_ne!(reference, "pic.jpg");
    assert_eq!(
        store
            .attachment(&owner.did_string(), &owner, "telegram", "42", reference)
            .unwrap()
            .1,
        b"photo bytes"
    );
}
