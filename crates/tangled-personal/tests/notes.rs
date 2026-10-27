use serde_json::{json, Value};
use tangled_personal::{notes::execute, Store};

fn call(store: &mut Store, input: Value) -> Value {
    execute(store, input).unwrap()
}

#[test]
fn private_note_lifecycle_preserves_extensions_and_successors() {
    let mut store = Store::memory();
    let first = call(
        &mut store,
        json!({"action":"create","rkey":"thought","body":{"text":"First","vendor":{"color":"blue"}}}),
    );
    assert_eq!(first["visibility"], "cap");
    let edited = call(
        &mut store,
        json!({"action":"edit","rkey":"thought","patch":{"title":"A thought","state":"draft"}}),
    );
    assert_eq!(edited["body"]["vendor"]["color"], "blue");
    assert_eq!(edited["body"]["text"], "First");
    assert_ne!(first["cid"], edited["cid"]);
    let got = call(&mut store, json!({"action":"get","rkey":"thought"}));
    assert_eq!(got["cid"], edited["cid"]);
    assert_eq!(
        call(&mut store, json!({"action":"list","state":"draft"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(&mut store, json!({"action":"delete","rkey":"thought"}));
    assert!(call(&mut store, json!({"action":"list"}))
        .as_array()
        .unwrap()
        .is_empty());
    assert!(execute(&mut store, json!({"action":"get","rkey":"thought"})).is_err());
}

#[test]
fn capture_append_remind_and_clear_reminder() {
    let mut store = Store::memory();
    call(
        &mut store,
        json!({"action":"capture","rkey":"inbox","body":{"text":"Remember","reminderAt":100}}),
    );
    call(
        &mut store,
        json!({"action":"append","rkey":"inbox","text":"Bring keys"}),
    );
    assert_eq!(
        call(&mut store, json!({"action":"get","rkey":"inbox"}))["body"]["text"],
        "Remember\nBring keys"
    );
    assert!(call(&mut store, json!({"action":"due","at":99}))
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        call(&mut store, json!({"action":"due","at":100}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(
        &mut store,
        json!({"action":"edit","rkey":"inbox","patch":{"reminderAt":null}}),
    );
    assert!(call(&mut store, json!({"action":"due","at":100}))
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn attachments_round_trip_as_private_note_blobs() {
    let mut store = Store::memory();
    call(
        &mut store,
        json!({"action":"create","rkey":"files","body":{"text":"Receipt"}}),
    );
    let note = call(
        &mut store,
        json!({"action":"attach","rkey":"files","name":"receipt.txt","mime":"text/plain","bytes":[104,105]}),
    );
    assert_eq!(note["visibility"], "cap");
    assert!(note["body"]["attachments"][0]["cid"].as_str().is_some());
    let attachment = call(
        &mut store,
        json!({"action":"attachment","rkey":"files","index":0}),
    );
    assert_eq!(attachment["bytes"], json!([104, 105]));
    assert_eq!(attachment["mime"], "text/plain");
}

#[test]
fn converting_saved_item_keeps_exact_evidence() {
    let mut store = Store::memory();
    let source = store
        .put(
            "pub.tangled.chat",
            "message",
            json!({"text":"A message"}),
            "cap",
        )
        .unwrap();
    let note = call(
        &mut store,
        json!({"action":"convert","rkey":"saved","source":{"collection":"pub.tangled.chat","rkey":"message"}}),
    );
    assert_eq!(note["body"]["text"], "A message");
    assert_eq!(note["body"]["provenance"]["uri"], source["uri"]);
    assert_eq!(note["body"]["provenance"]["cid"], source["cid"]);
    assert_eq!(note["visibility"], "cap");
    assert!(execute(
        &mut store,
        json!({"action":"edit","rkey":"saved","patch":{"provenance":null}})
    )
    .is_err());
}

#[test]
fn invalid_inputs_do_not_change_notes() {
    let mut store = Store::memory();
    for input in [
        json!(null),
        json!({"action":"unknown"}),
        json!({"action":"create","body":{"text":4}}),
        json!({"action":"create","body":{"text":"x","state":"other"}}),
        json!({"action":"create","body":{"text":"x","reminderAt":-1}}),
        json!({"action":"create","body":{"text":"x","attachments":[{"cid":"not-a-cid","mime":"x","name":"x"}]}}),
    ] {
        assert!(execute(&mut store, input).is_err());
    }
    call(
        &mut store,
        json!({"action":"create","rkey":"existing","body":{"text":"Keep"}}),
    );
    assert!(execute(
        &mut store,
        json!({"action":"create","rkey":"existing","body":{"text":"Replace"}})
    )
    .is_err());
    assert!(execute(
        &mut store,
        json!({"action":"edit","rkey":"existing","patch":{"text":12}})
    )
    .is_err());
    assert!(execute(
        &mut store,
        json!({"action":"edit","rkey":"existing","patch":{"createdAt":0}})
    )
    .is_err());
    assert_eq!(
        call(&mut store, json!({"action":"get","rkey":"existing"}))["body"]["text"],
        "Keep"
    );
}

#[test]
fn generated_keys_and_empty_drafts_are_usable() {
    let mut store = Store::memory();
    let draft = call(
        &mut store,
        json!({"action":"create","body":{"text":"","state":"draft"}}),
    );
    assert!(draft["uri"]
        .as_str()
        .unwrap()
        .contains("/pub.tangled.note/"));
    call(
        &mut store,
        json!({"action":"capture","body":{"text":"New idea"}}),
    );
    assert_eq!(
        call(&mut store, json!({"action":"list"}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
