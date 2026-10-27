use serde_json::{json, Value};
use tangled_personal::{connections::execute, Store};

fn call(store: &mut Store, input: Value) -> Value {
    execute(store, input).unwrap()
}
fn item(source: &str, account: &str, conversation: &str, text: &str, revision: u64) -> Value {
    json!({"key":"archive-key", "source":source,"account":account,"vendor":{"retained":true},
        "object":{"id":"42","kind":"message","conversation":conversation,"sender":"native-alice",
        "timestamp":"2026-01-01T10:00:00Z","text":text,"attachments":["native-file"],
        "data":{"source_revision":revision,"unknown":[1,2]},"extension":"keep"}})
}
fn ingest(store: &mut Store, row: Value) -> Value {
    call(store, json!({"action":"ingest","activity":[row]}))["items"][0].clone()
}
fn key(record: &Value) -> &str {
    record["rkey"].as_str().unwrap()
}

#[test]
fn recovery_activity_keeps_native_identity_original_and_private_visibility() {
    let mut store = Store::memory();
    let row = item("telegram", "me", "room", "hello", 1);
    let record = ingest(&mut store, row.clone());
    assert_eq!(record["vis"], "cap");
    assert_eq!(record["body"]["imported"], row);
    assert_eq!(
        record["body"]["identity"],
        json!({"source":"telegram","account":"me","kind":"message","conversation":"room","id":"42"})
    );
    assert_eq!(record["author"], store.did());
    let original = call(
        &mut store,
        json!({"action":"original","rkey":key(&record),"index":0}),
    );
    let bytes: Vec<u8> = original["bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), row);
    assert_eq!(ingest(&mut store, row)["cid"], record["cid"]);
    assert!(store.list(Some("pub.tangled.post")).unwrap().is_empty());
}

#[test]
fn source_account_and_conversation_are_isolated_and_searchable() {
    let mut store = Store::memory();
    for (s, a, c, t) in [
        ("telegram", "me", "one", "find me"),
        ("telegram", "other", "one", "hello"),
        ("telegram", "me", "two", "hello"),
        ("whatsapp", "me", "one", "hello"),
    ] {
        ingest(&mut store, item(s, a, c, t, 1));
    }
    assert_eq!(
        call(&mut store, json!({"action":"list"}))
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(call(&mut store,json!({"action":"search","source":"telegram","account":"me","conversation":"one","query":"FIND"})).as_array().unwrap().len(),1);
    assert_eq!(
        call(
            &mut store,
            json!({"action":"list","source":"whatsapp","sender":"native-alice"})
        )
        .as_array()
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn revisions_reject_rollback_keep_unknown_fields_and_show_source_deletion() {
    let mut store = Store::memory();
    let first = ingest(&mut store, item("telegram", "me", "room", "old", 1));
    let mut body = first["body"].clone();
    body["appExtension"] = json!({"color":"blue"});
    store
        .put("pub.tangled.sourceItem", key(&first), body, "cap")
        .unwrap();
    let newer = ingest(&mut store, item("telegram", "me", "room", "new", 3));
    assert_eq!(newer["body"]["appExtension"]["color"], "blue");
    let stale = ingest(&mut store, item("telegram", "me", "room", "stale", 2));
    assert_eq!(stale["body"]["imported"]["object"]["text"], "new");
    assert_eq!(stale["body"]["revisions"].as_array().unwrap().len(), 3);
    let mut deleted = item("telegram", "me", "room", "new", 4);
    deleted["object"]["deleted"] = json!(true);
    let deleted = ingest(&mut store, deleted);
    assert_eq!(deleted["body"]["deleted"], true);
    assert!(call(&mut store, json!({"action":"list"}))
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        call(&mut store, json!({"action":"list","includeDeleted":true}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&mut store, json!({"action":"history","rkey":key(&deleted)}))
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn opaque_edits_require_explicit_revision_selection() {
    let mut store = Store::memory();
    let mut row = item("custom", "me", "room", "first", 1);
    row["object"]["data"] = json!({"revision":"opaque-a"});
    let first = ingest(&mut store, row.clone());
    row["object"]["text"] = json!("edited");
    let changed = ingest(&mut store, row);
    assert_eq!(changed["body"]["imported"]["object"]["text"], "first");
    let selected = call(
        &mut store,
        json!({"action":"select-revision","rkey":key(&first),"index":1}),
    );
    assert_eq!(selected["body"]["imported"]["object"]["text"], "edited");
}

#[test]
fn overlays_and_contact_groups_do_not_rewrite_native_authors() {
    let mut store = Store::memory();
    let first = ingest(&mut store, item("telegram", "me", "room", "hello", 1));
    call(
        &mut store,
        json!({"action":"overlay","rkey":key(&first),"patch":{"bookmarked":true,"note":"Remember","tags":["work"],"app":{"x":1}}}),
    );
    let overlay = call(
        &mut store,
        json!({"action":"overlay","rkey":key(&first),"patch":{"tags":["later"]}}),
    );
    assert_eq!(overlay["body"]["app"]["x"], 1);
    assert_eq!(overlay["vis"], "cap");
    for source in ["telegram", "whatsapp"] {
        call(
            &mut store,
            json!({"action":"contact","source":source,"account":"me","sender":"native-alice","patch":{"alias":"Alice","groups":["friends"],"app":{"color":"blue"}}}),
        );
    }
    let contact = call(
        &mut store,
        json!({"action":"contact","source":"telegram","account":"me","sender":"native-alice","patch":{"alias":"A"}}),
    );
    assert_eq!(contact["body"]["app"]["color"], "blue");
    assert_eq!(
        call(&mut store, json!({"action":"contacts","group":"friends"}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let found = call(
        &mut store,
        json!({"action":"list","bookmarked":true,"tag":"later","contact":contact["uri"]}),
    );
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(
        found[0]["body"]["imported"]["object"]["sender"],
        "native-alice"
    );
    assert_eq!(
        store.get("pub.tangled.sourceItem", key(&first)).unwrap()["cid"],
        first["cid"]
    );
}

#[test]
fn reply_export_requires_review_of_exact_account_recipient_and_current_preview() {
    let mut store = Store::memory();
    let first = ingest(&mut store, item("telegram", "me", "room", "hello", 1));
    assert!(execute(&mut store,json!({"action":"reply-preview","rkey":key(&first),"text":"reply","account":"other","recipient":"native-alice"})).is_err());
    let preview = call(
        &mut store,
        json!({"action":"reply-preview","rkey":key(&first),"text":"reply","account":"me","recipient":"native-alice"}),
    );
    assert_eq!(preview["body"]["reply"]["text"], "reply");
    assert!(execute(
        &mut store,
        json!({"action":"reply-export","rkey":key(&first),"previewCid":preview["cid"]})
    )
    .is_err());
    let intent = call(
        &mut store,
        json!({"action":"reply-export","rkey":key(&first),"previewCid":preview["cid"],"confirmed":true}),
    );
    assert_eq!(intent["source"], "telegram");
    assert_eq!(intent["account"], "me");
    assert_eq!(intent["recipient"], "native-alice");
    assert_eq!(intent["conversation"], "room");
    assert_eq!(intent["inReplyTo"], "42");
    assert_eq!(intent["delivery"], "not-sent");
    ingest(&mut store, item("telegram", "me", "room", "edited", 2));
    assert!(execute(&mut store,json!({"action":"reply-export","rkey":key(&first),"previewCid":preview["cid"],"confirmed":true})).is_err());
    assert!(store.list(Some("pub.tangled.chat")).unwrap().is_empty());
    assert!(store.list(Some("pub.tangled.post")).unwrap().is_empty());
    assert_eq!(store.list(None).unwrap().len(), 2);
}

#[test]
fn malformed_activity_and_credential_bundles_are_rejected_before_any_writes() {
    let mut store = Store::memory();
    let good = item("telegram", "me", "room", "hello", 1);
    let mut credentials = good.clone();
    credentials["object"]["data"]["access_token"] = json!("secret");
    for input in [
        json!(null),
        json!({"action":"ingest","activity":[good,credentials]}),
        json!({"action":"ingest","activity":{"ed25519":"secret"}}),
        json!({"action":"ingest","activity":[{"source":"telegram","account":"me","object":{"id":"42"}}]}),
    ] {
        assert!(execute(&mut store, input).is_err());
    }
    assert!(store.list(None).unwrap().is_empty());
}

#[test]
fn supplied_original_bytes_round_trip_and_unbookmarked_items_are_queryable() {
    let mut store = Store::memory();
    let mut row = item("custom", "me", "room", "text", 1);
    row["original"] = json!({"name":"export.txt","mime":"text/plain","bytes":[0,255,10,32]});
    let record = ingest(&mut store, row);
    let original = call(
        &mut store,
        json!({"action":"original","rkey":key(&record),"index":0,"sourceOriginal":true}),
    );
    assert_eq!(original["bytes"], json!([0, 255, 10, 32]));
    assert_eq!(original["mime"], "text/plain");
    assert_eq!(
        call(&mut store, json!({"action":"list","bookmarked":false}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(execute(
        &mut store,
        json!({"action":"overlay","rkey":key(&record),"patch":{"target":"elsewhere"}})
    )
    .is_err());
    assert!(execute(&mut store,json!({"action":"contact","source":"custom","account":"me","sender":"native-alice","patch":{"identity":{}}})).is_err());
}

#[test]
fn editing_a_preview_invalidates_prior_confirmation_without_altering_source() {
    let mut store = Store::memory();
    let record = ingest(&mut store, item("telegram", "me", "room", "hello", 1));
    let preview = call(
        &mut store,
        json!({"action":"reply-preview","rkey":key(&record),"account":"me","recipient":"native-alice","text":"first"}),
    );
    let edited = call(
        &mut store,
        json!({"action":"reply-preview","rkey":key(&record),"account":"me","recipient":"native-alice","text":"second"}),
    );
    assert!(execute(&mut store,json!({"action":"reply-export","rkey":key(&record),"previewCid":preview["cid"],"confirmed":true})).is_err());
    let intent = call(
        &mut store,
        json!({"action":"reply-export","rkey":key(&record),"previewCid":edited["cid"],"confirmed":true}),
    );
    assert_eq!(intent["text"], "second");
    assert_eq!(
        store.get("pub.tangled.sourceItem", key(&record)).unwrap()["cid"],
        record["cid"]
    );
}
