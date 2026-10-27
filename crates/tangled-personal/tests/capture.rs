use serde_json::{json, Value};
use tangled_personal::{capture::execute, Store};

fn save(store: &mut Store, input: Value) -> Value {
    execute(store, input).expect("save capture")
}

#[test]
fn bookmark_is_private_and_duplicate_save_does_not_erase_user_edits() {
    let mut store = Store::memory();
    let first = save(
        &mut store,
        json!({"action":"save","url":"https://example.test/story","title":"Story","source":{"type":"browser","selection":"quote"}}),
    );
    assert_eq!(first["vis"], "cap");
    assert_eq!(first["body"]["kind"], "bookmark");
    let uri = first["uri"].as_str().unwrap();
    let edited = execute(
        &mut store,
        json!({"action":"update","uri":uri,"patch":{"title":"My title","custom":{"keep":true}}}),
    )
    .unwrap();
    let again = save(
        &mut store,
        json!({"action":"save","url":"https://example.test/story","title":"Other title"}),
    );
    assert_eq!(again["cid"], edited["cid"]);
    assert_eq!(again["body"]["title"], "My title");
    assert_eq!(again["body"]["custom"], json!({"keep":true}));
    assert_eq!(again["body"]["source"]["selection"], "quote");
    assert_eq!(
        execute(&mut store, json!({"action":"list"}))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn article_reader_uses_saved_plain_text_offline_and_preserves_extensions() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let item = save(
        &mut store,
        json!({"action":"save","kind":"article","url":"https://example.test/read","title":"Saved","text":"Article\n<script>alert(1)</script>","extensions":{"example.extra":{"v":1}}}),
    );
    let uri = item["uri"].as_str().unwrap();
    assert!(item["body"]["blob"].is_string());
    let exported = store.export().unwrap();
    store.import(&exported).unwrap();
    drop(store);
    let mut store = Store::open(directory.path()).unwrap();
    // Reading the signed content needs no source request or executing renderer.
    let reader = execute(&mut store, json!({"action":"render","uri":uri})).unwrap();
    assert_eq!(reader["mime"], "text/plain");
    assert_eq!(reader["text"], "Article\n<script>alert(1)</script>");
    let updated = execute(
        &mut store,
        json!({"action":"update","uri":uri,"patch":{"text":"Updated article","title":"Revised"}}),
    )
    .unwrap();
    assert_eq!(updated["body"]["example.extra"], json!({"v":1}));
    assert_ne!(updated["body"]["blob"], item["body"]["blob"]);
    assert_eq!(
        execute(&mut store, json!({"action":"read","uri":uri})).unwrap()["text"],
        "Updated article"
    );
}

#[test]
fn tags_and_read_star_filters_follow_explicit_mutations() {
    let mut store = Store::memory();
    let item = save(
        &mut store,
        json!({"action":"save","kind":"text","text":"Remember this","tags":["work","work"]}),
    );
    let uri = item["uri"].as_str().unwrap();
    execute(
        &mut store,
        json!({"action":"tag","uri":uri,"tags":["later","work","later"]}),
    )
    .unwrap();
    execute(&mut store, json!({"action":"mark-read","uri":uri})).unwrap();
    let starred = execute(
        &mut store,
        json!({"action":"star","uri":uri,"starred":true}),
    )
    .unwrap();
    assert_eq!(starred["body"]["tags"], json!(["later", "work"]));
    assert_eq!(
        execute(
            &mut store,
            json!({"action":"list","tag":"later","read":true,"starred":true})
        )
        .unwrap()
        .as_array()
        .unwrap()
        .len(),
        1
    );
    execute(&mut store, json!({"action":"mark-unread","uri":uri})).unwrap();
    assert!(execute(&mut store, json!({"action":"list","read":true}))
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn binary_screenshot_round_trips_blob_and_deletes_signed_item() {
    let mut store = Store::memory();
    let item = save(
        &mut store,
        json!({"action":"save","kind":"screenshot","title":"Screen","mime":"image/png","bytes":[137,80,78,71,0,255]}),
    );
    let uri = item["uri"].as_str().unwrap();
    let blob = item["body"]["blob"].as_str().unwrap();
    assert_eq!(store.get_blob(blob).unwrap(), vec![137, 80, 78, 71, 0, 255]);
    assert_eq!(
        execute(&mut store, json!({"action":"download","uri":uri})).unwrap()["bytes"],
        json!([137, 80, 78, 71, 0, 255])
    );
    let reader = execute(&mut store, json!({"action":"read","uri":uri})).unwrap();
    assert_eq!(reader["mime"], "text/plain");
    assert!(reader["text"].as_str().unwrap().contains("image/png"));
    execute(&mut store, json!({"action":"delete","uri":uri})).unwrap();
    assert!(execute(&mut store, json!({"action":"get","uri":uri})).is_err());
    assert!(execute(&mut store, json!({"action":"list"}))
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn text_file_is_read_as_plain_text_and_content_deduplicates_without_url() {
    let mut store = Store::memory();
    let input = json!({"action":"save","kind":"file","mime":"text/html","bytes":[60,98,62,72,105,60,47,98,62]});
    let a = save(&mut store, input.clone());
    let b = save(&mut store, input);
    assert_eq!(a["cid"], b["cid"]);
    let reader = execute(&mut store, json!({"action":"read","uri":a["uri"]})).unwrap();
    assert_eq!(reader["text"], "<b>Hi</b>");
    assert_eq!(reader["mime"], "text/plain");
}

#[test]
fn invalid_input_is_rejected_before_writing_any_record() {
    let mut store = Store::memory();
    for input in [
        json!({"action":"save","url":"javascript:alert(1)"}),
        json!({"action":"save","kind":"file","bytes":[256],"mime":"image/png"}),
        json!({"action":"save","kind":"article","title":"No text"}),
        json!({"action":"save","text":"secret","visibility":"public"}),
        json!({"action":"save","text":"x","tags":"wrong"}),
        json!({"action":"save","text":"x","read":"true"}),
        json!({"action":"save","kind":"screenshot","bytes":[1],"mime":"text/html"}),
    ] {
        assert!(execute(&mut store, input).is_err());
    }
    assert!(execute(&mut store, json!({"action":"list"}))
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn invalid_update_preserves_record_and_rejects_identity_or_visibility_changes() {
    let mut store = Store::memory();
    let item = save(
        &mut store,
        json!({"action":"save","text":"Saved","extensions":{"futureField":42}}),
    );
    for patch in [
        json!({"blob":"bad"}),
        json!({"url":"https://other.test"}),
        json!({"visibility":"public"}),
        json!({"read":"false"}),
        json!({"title":null}),
    ] {
        assert!(execute(
            &mut store,
            json!({"action":"update","uri":item["uri"],"patch":patch})
        )
        .is_err());
    }
    let read = execute(&mut store, json!({"action":"get","uri":item["uri"]})).unwrap();
    assert_eq!(read["cid"], item["cid"]);
    assert_eq!(read["body"]["futureField"], 42);
}
