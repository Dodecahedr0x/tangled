use serde_json::{json, Value};
use tangled_personal::{memory::execute, Store};

fn put(store: &mut Store, key: &str, body: Value) -> Value {
    store.put("org.example.journal", key, body, "cap").unwrap()
}

#[test]
fn searches_unknown_collections_with_unicode_and_exact_evidence() {
    let mut store = Store::memory();
    let record = put(
        &mut store,
        "one",
        json!({"title":"Été à 東京", "text":"Café with Alice", "source":"mail", "tags":["travel"]}),
    );
    put(
        &mut store,
        "two",
        json!({"title":"Other", "text":"Alice alone"}),
    );
    let result = execute(&mut store, json!({"action":"search", "query":"CAFÉ 東京"})).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["results"][0]["uri"], record["uri"]);
    assert_eq!(result["results"][0]["cid"], record["cid"]);
    assert!(result["results"][0]["snippet"]
        .as_str()
        .unwrap()
        .contains("東京"));
}

#[test]
fn excludes_secret_material_and_deleted_records() {
    let mut store = Store::memory();
    put(
        &mut store,
        "secret",
        json!({"text":"ordinary", "password":"needle", "signature":"needle", "ciphertext":"needle", "credentials":{"text":"needle"}}),
    );
    put(&mut store, "gone", json!({"text":"needle"}));
    store.remove("org.example.journal", "gone").unwrap();
    let result = execute(&mut store, json!({"action":"search", "query":"needle"})).unwrap();
    assert_eq!(result["total"], 0);
}

#[test]
fn combines_source_collection_tag_and_personal_group_filters() {
    let mut store = Store::memory();
    let record = put(
        &mut store,
        "one",
        json!({"text":"project horizon", "source":{"kind":"email", "url":"https://mail.example/1"}}),
    );
    put(
        &mut store,
        "two",
        json!({"text":"project horizon", "source":"email", "tags":["work"]}),
    );
    let annotation = execute(&mut store, json!({"action":"annotate", "rkey":"a", "text":"Ask about budget", "target":{"uri":record["uri"], "cid":record["cid"]}, "tags":["work"]})).unwrap();
    assert_eq!(annotation["vis"], "cap");
    execute(&mut store, json!({"action":"put-collection", "rkey":"projects", "name":"Projects", "items":[{"uri":record["uri"], "cid":record["cid"]}]})).unwrap();
    let result = execute(&mut store, json!({"action":"search", "query":"budget", "source":"email", "collection":"org.example.journal", "tag":"work", "group":"projects"})).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["results"][0]["uri"], record["uri"]);
}

#[test]
fn saved_searches_recompute_and_preserve_extension_fields_on_edit() {
    let mut store = Store::memory();
    store
        .put(
            "pub.tangled.savedSearch",
            "s",
            json!({"name":"Old", "query":"old", "x-custom":{"retain":true}}),
            "cap",
        )
        .unwrap();
    execute(
        &mut store,
        json!({"action":"save-search", "rkey":"s", "name":"Cafés", "query":"café", "tag":"travel"}),
    )
    .unwrap();
    assert_eq!(
        store.get("pub.tangled.savedSearch", "s").unwrap()["body"]["x-custom"]["retain"],
        true
    );
    assert_eq!(
        execute(&mut store, json!({"action":"run-search", "rkey":"s"})).unwrap()["total"],
        0
    );
    put(&mut store, "new", json!({"text":"café", "tags":["travel"]}));
    assert_eq!(
        execute(&mut store, json!({"action":"run-search", "rkey":"s"})).unwrap()["total"],
        1
    );
    assert_eq!(
        execute(&mut store, json!({"action":"list-searches"})).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    execute(&mut store, json!({"action":"delete-search", "rkey":"s"})).unwrap();
    assert!(execute(&mut store, json!({"action":"run-search", "rkey":"s"})).is_err());
}

#[test]
fn metadata_edits_preserve_unknown_fields_and_reject_invalid_inputs() {
    let mut store = Store::memory();
    let record = put(&mut store, "one", json!({"text":"target"}));
    store
        .put(
            "pub.tangled.annotation",
            "a",
            json!({"text":"before", "target":{"uri":record["uri"]}, "extension":[1,2]}),
            "cap",
        )
        .unwrap();
    let edited = execute(
        &mut store,
        json!({"action":"annotate", "rkey":"a", "text":"after"}),
    )
    .unwrap();
    assert_eq!(edited["body"]["extension"], json!([1, 2]));
    assert_eq!(edited["body"]["target"]["uri"], record["uri"]);
    for input in [
        json!(null),
        json!({"action":"bad"}),
        json!({"action":"search", "limit":0}),
        json!({"action":"annotate", "rkey":"bad", "text":"x", "target":{"uri":"https://example.com"}}),
        json!({"action":"search", "tag":["bad"]}),
    ] {
        assert!(execute(&mut store, input).is_err());
    }
}

#[test]
fn deterministic_pagination_uses_current_record_cids() {
    let mut store = Store::memory();
    put(&mut store, "b", json!({"text":"topic"}));
    put(&mut store, "a", json!({"text":"topic"}));
    let first = execute(
        &mut store,
        json!({"action":"search", "query":"topic", "limit":1}),
    )
    .unwrap();
    let second = execute(
        &mut store,
        json!({"action":"search", "query":"topic", "limit":1, "offset":1}),
    )
    .unwrap();
    assert_eq!(first["total"], 2);
    assert!(first["results"][0]["uri"].as_str().unwrap().ends_with("/a"));
    assert!(second["results"][0]["uri"]
        .as_str()
        .unwrap()
        .ends_with("/b"));
    let updated = put(&mut store, "a", json!({"text":"topic updated"}));
    assert_eq!(
        execute(&mut store, json!({"action":"search", "query":"updated"})).unwrap()["results"][0]
            ["cid"],
        updated["cid"]
    );
}

#[test]
fn annotations_make_nontext_sources_searchable_until_deleted() {
    let mut store = Store::memory();
    let record = put(&mut store, "photo", json!({"blob":"opaque-blob-reference"}));
    execute(&mut store, json!({"action":"annotate", "rkey":"photo-note", "text":"Bridge in Kyoto", "target":{"uri":record["uri"]}})).unwrap();
    let query = json!({"action":"search", "query":"Kyoto", "collection":"org.example.journal"});
    assert_eq!(execute(&mut store, query.clone()).unwrap()["total"], 1);
    execute(
        &mut store,
        json!({"action":"delete-annotation", "rkey":"photo-note"}),
    )
    .unwrap();
    assert_eq!(execute(&mut store, query).unwrap()["total"], 0);
}

#[test]
fn snippets_show_late_unicode_matches_without_splitting_characters() {
    let mut store = Store::memory();
    put(
        &mut store,
        "long",
        json!({"text":format!("{} 東京 meeting", "é".repeat(400))}),
    );
    let result = execute(&mut store, json!({"action":"search", "query":"東京"})).unwrap();
    let snippet = result["results"][0]["snippet"].as_str().unwrap();
    assert!(snippet.contains("東京"));
    assert!(snippet.chars().count() <= 240);
}

#[test]
fn validates_optional_metadata_and_excludes_encrypted_content() {
    let mut store = Store::memory();
    let record = put(
        &mut store,
        "encrypted",
        json!({"encrypted":true, "text":"hiddenword"}),
    );
    assert_eq!(
        execute(&mut store, json!({"action":"search", "query":"hiddenword"})).unwrap()["total"],
        0
    );
    assert!(execute(&mut store, json!({"action":"annotate", "rkey":"invalid", "text":"note", "quote":42, "target":{"uri":record["uri"]}})).is_err());
    assert!(execute(
        &mut store,
        json!({"action":"put-collection", "rkey":"invalid", "name":"Group", "description":42})
    )
    .is_err());
}

#[test]
fn indexes_capture_article_blobs_and_feed_item_bodies_with_exact_evidence() {
    let mut store = Store::memory();
    let article = tangled_personal::capture::execute(
        &mut store,
        json!({"action":"save","kind":"article","title":"Article","text":"uniquebodyterm 東京"}),
    )
    .unwrap();
    let result = execute(
        &mut store,
        json!({"action":"search","query":"uniquebodyterm"}),
    )
    .unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["results"][0]["uri"], article["uri"]);
    assert_eq!(result["results"][0]["cid"], article["cid"]);
    assert!(result["results"][0]["snippet"]
        .as_str()
        .unwrap()
        .contains("uniquebodyterm"));
    tangled_personal::feeds::execute(
        &mut store,
        json!({"action":"subscribe","url":"https://example.org/feed"}),
    )
    .unwrap();
    let feed = tangled_personal::feeds::execute(&mut store, json!({"action":"ingest","url":"https://example.org/feed","xml":"<rss><channel><item><guid>id1</guid><title>Feed title</title><description>&lt;p&gt;uniquefeedbodyterm&lt;/p&gt;</description></item></channel></rss>"})).unwrap();
    let result = execute(
        &mut store,
        json!({"action":"search","query":"uniquefeedbodyterm"}),
    )
    .unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["results"][0]["cid"], feed["items"][0]["cid"]);
    let snippet = result["results"][0]["snippet"].as_str().unwrap();
    assert!(snippet.contains("uniquefeedbodyterm"));
    assert!(!snippet.contains('<'));
}

#[test]
fn known_content_adapters_exclude_credentials_binary_encryption_and_html_code() {
    let mut store = Store::memory();
    let secret = store.put_blob(b"credentialneedle", "text/plain").unwrap();
    let binary = store.put_blob(&[0xff, 0xfe], "text/plain").unwrap();
    let oversized = store
        .put_blob("oversizedneedle".repeat(20000).as_bytes(), "text/plain")
        .unwrap();
    for (key, collection, body) in [
        ("unknown", "org.example.file", json!({"blob":secret})),
        (
            "attachment",
            "pub.tangled.capture",
            json!({"kind":"file","mime":"text/plain","blob":secret}),
        ),
        (
            "wrong-mime",
            "pub.tangled.capture",
            json!({"kind":"article","mime":"application/octet-stream","blob":secret}),
        ),
        (
            "encrypted",
            "pub.tangled.capture",
            json!({"kind":"article","mime":"text/plain","blob":secret,"encrypted":true}),
        ),
        (
            "binary",
            "pub.tangled.capture",
            json!({"kind":"article","mime":"text/plain","blob":binary}),
        ),
        (
            "oversized",
            "pub.tangled.capture",
            json!({"kind":"article","mime":"text/plain","blob":oversized}),
        ),
        (
            "rich",
            "pub.tangled.feedItem",
            json!({"body":"credentialneedle","bodyType":"credential"}),
        ),
        (
            "html",
            "pub.tangled.feedItem",
            json!({"body":"<script>credentialneedle</script><style>credentialneedle</style><p data-key='credentialneedle'>safe visiblefeedword</p>","bodyType":"html"}),
        ),
    ] {
        store.put(collection, key, body, "cap").unwrap();
    }
    for query in ["credentialneedle", "oversizedneedle"] {
        assert_eq!(
            execute(&mut store, json!({"action":"search","query":query})).unwrap()["total"],
            0
        );
    }
    let found = execute(
        &mut store,
        json!({"action":"search","query":"visiblefeedword"}),
    )
    .unwrap();
    assert_eq!(found["total"], 1);
    assert!(!found["results"][0]["snippet"]
        .as_str()
        .unwrap()
        .contains('<'));
}
