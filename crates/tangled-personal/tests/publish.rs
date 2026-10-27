use serde_json::{json, Value};
use tangled_personal::{publish::execute, Store};

fn source(store: &mut Store, key: &str, body: Value, visibility: &str) -> Value {
    store
        .put("pub.example.story", key, body, visibility)
        .unwrap()
}

#[test]
fn private_export_requires_explicit_confirmation_and_does_not_change_source() {
    let mut store = Store::memory();
    let original = source(
        &mut store,
        "one",
        json!({"title":"Private", "text":"Chosen words"}),
        "cap",
    );
    let request = json!({"action":"render", "records":[{"uri":original["uri"]}]});
    assert!(execute(&mut store, request.clone()).is_err());
    let mut confirmed = request;
    confirmed["confirmPublish"] = json!(true);
    let output = execute(&mut store, confirmed).unwrap();
    assert!(output["html"].as_str().unwrap().contains("Chosen words"));
    assert_eq!(store.get("pub.example.story", "one").unwrap(), original);
    assert_eq!(store.list(None).unwrap().len(), 1);
}

#[test]
fn unknown_client_content_is_escaped_and_arbitrary_metadata_is_never_exported() {
    let mut store = Store::memory();
    source(
        &mut store,
        "chosen",
        json!({"title":"<script>alert(1)</script>", "text":"A & B <svg onload=evil()> ", "description":"O'Reilly", "source":{"token":"SOURCE_SECRET"}, "extension":{"text":"EXTENSION_SECRET"}, "url":"https://private.example/SECRET", "blob":"PRIVATE_CID", "attachments":[{"blob":"HIDDEN_ATTACHMENT"}]}),
        "public",
    );
    source(
        &mut store,
        "other",
        json!({"title":"UNSELECTED_SECRET"}),
        "cap",
    );
    let output = execute(&mut store, json!({"action":"render", "title":"My & site", "records":[{"collection":"pub.example.story", "rkey":"chosen"}]})).unwrap();
    let serialized = output.to_string();
    for secret in [
        "SOURCE_SECRET",
        "EXTENSION_SECRET",
        "PRIVATE_CID",
        "UNSELECTED_SECRET",
        "HIDDEN_ATTACHMENT",
        "private.example",
        "tangled://",
    ] {
        assert!(!serialized.contains(secret), "leaked {secret}");
    }
    let html = output["html"].as_str().unwrap();
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<svg"));
    assert!(html.contains("default-src"));
    let rss = output["rss"].as_str().unwrap();
    assert!(rss.contains("<rss version=\"2.0\">"));
    assert!(rss.contains("My &amp; site"));
    assert!(!rss.contains("<script>"));
}

#[test]
fn original_blob_bytes_are_included_only_when_individually_selected() {
    let mut store = Store::memory();
    let blob = store
        .put_blob(b"<svg onload='evil()'/>", "image/svg+xml")
        .unwrap();
    let text_blob = store
        .put_blob(b"Full body & literal <b>text</b>", "text/plain")
        .unwrap();
    source(
        &mut store,
        "one",
        json!({"title":"Files", "blob":blob}),
        "cap",
    );
    let output = execute(&mut store, json!({"action":"export", "confirmPublish":true, "records":[{"collection":"pub.example.story", "rkey":"one", "textBlob":text_blob, "attachments":[{"cid":blob,"name":"Diagram <svg>"}]}]})).unwrap();
    assert!(output["html"]
        .as_str()
        .unwrap()
        .contains("Full body &amp; literal &lt;b&gt;text&lt;/b&gt;"));
    let files = output["files"].as_array().unwrap();
    let attachment = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with(".bin"))
        .unwrap();
    assert_eq!(
        attachment["bytes"],
        json!(b"<svg onload='evil()'/>".to_vec())
    );
    assert_eq!(attachment["mediaType"], "application/octet-stream");
    assert!(!output["html"].as_str().unwrap().contains("<svg"));
    assert!(output["html"].as_str().unwrap().contains("download"));
    assert_eq!(store.list(None).unwrap().len(), 1);
}

#[test]
fn public_publication_is_a_stable_signed_copy_and_edit_preserves_extensions() {
    let mut store = Store::memory();
    let original = source(
        &mut store,
        "one",
        json!({"title":"Story", "text":"First"}),
        "cap",
    );
    let mut request = json!({"action":"publish", "confirmPublish":true, "key":"newsletter", "title":"News", "records":[{"uri":original["uri"]}]});
    let first = execute(&mut store, request.clone()).unwrap();
    let first_record = &first["publication"];
    assert_eq!(first_record["vis"], "public");
    assert_eq!(first_record["collection"], "pub.tangled.publication");
    assert_eq!(first_record["body"]["items"][0]["text"], "First");
    assert!(first_record["sig"].as_str().unwrap().len() == 128);
    assert!(!first_record["body"]
        .to_string()
        .contains(original["uri"].as_str().unwrap()));
    let mut body = first_record["body"].clone();
    body["vendor.example"] = json!({"keep":true});
    store
        .put("pub.tangled.publication", "newsletter", body, "public")
        .unwrap();
    request["title"] = json!("Revised");
    let second = execute(&mut store, request).unwrap();
    assert_eq!(second["publication"]["uri"], first_record["uri"]);
    assert_ne!(second["publication"]["cid"], first_record["cid"]);
    assert_eq!(
        second["publication"]["body"]["vendor.example"]["keep"],
        true
    );
    assert_eq!(second["publication"]["body"]["title"], "Revised");
    assert_eq!(store.get("pub.example.story", "one").unwrap()["vis"], "cap");
    let html_cid = second["publication"]["body"]["html"].as_str().unwrap();
    assert_eq!(
        String::from_utf8(store.get_blob(html_cid).unwrap()).unwrap(),
        second["html"]
    );
}

#[test]
fn selected_collection_exports_static_site_without_account_or_source_metadata() {
    let mut store = Store::memory();
    source(
        &mut store,
        "a",
        json!({"title":"A", "text":"Alpha"}),
        "public",
    );
    source(
        &mut store,
        "b",
        json!({"title":"B", "description":"Beta"}),
        "public",
    );
    store
        .put(
            "pub.example.other",
            "secret",
            json!({"text":"HIDDEN"}),
            "cap",
        )
        .unwrap();
    let parent = tempfile::tempdir().unwrap();
    let directory = parent.path().join("share");
    let output = execute(
        &mut store,
        json!({"action":"export", "collection":"pub.example.story", "outputDir":directory}),
    )
    .unwrap();
    assert_eq!(output["count"], 2);
    let html = std::fs::read_to_string(directory.join("index.html")).unwrap();
    assert!(html.contains("Alpha"));
    assert!(html.contains("Beta"));
    assert!(!html.contains("HIDDEN"));
    assert_eq!(
        std::fs::read_to_string(directory.join("feed.xml")).unwrap(),
        output["rss"]
    );
    assert!(execute(
        &mut store,
        json!({"action":"export", "collection":"pub.example.story", "outputDir":directory})
    )
    .is_err());
}

#[test]
fn invalid_selection_and_unsafe_links_fail_before_public_writes() {
    let mut store = Store::memory();
    let original = source(&mut store, "a", json!({"text":"A"}), "public");
    for request in [
        json!({"action":"publish", "confirmPublish":true, "key":"x"}),
        json!({"action":"render", "records":[]}),
        json!({"action":"render", "records":[{"uri":"tangled://did:sol:foreign/pub.example.story/a"}]}),
        json!({"action":"render", "records":[{"uri":original["uri"], "link":"javascript:alert(1)"}]}),
        json!({"action":"render", "records":[{"uri":original["uri"], "link":"https://user:password@example.org/"}]}),
        json!({"action":"publish", "key":"../bad", "confirmPublish":true, "records":[{"uri":original["uri"]}]}),
        json!({"action":"render", "collection":"pub.example.story", "records":[{"uri":original["uri"]}]}),
        json!({"action":"publish", "key":"x", "records":[{"uri":original["uri"]}]}),
    ] {
        assert!(
            execute(&mut store, request.clone()).is_err(),
            "accepted {request}"
        );
        assert_eq!(store.list(None).unwrap().len(), 1);
    }
    let output = execute(&mut store, json!({"action":"render", "records":[{"uri":original["uri"], "link":"https://example.org/story?a=1&b=2"}]})).unwrap();
    assert!(output["html"]
        .as_str()
        .unwrap()
        .contains("href=\"https://example.org/story?a=1&amp;b=2\""));
}

#[test]
fn rss_description_cannot_become_active_html_after_xml_decoding() {
    let mut store = Store::memory();
    source(
        &mut store,
        "one",
        json!({"title":"Control\u{0000}", "text":"<img src=x onerror=evil()> & literal"}),
        "public",
    );
    let output = execute(&mut store, json!({"action":"render", "description":"<script>channel()</script>", "records":[{"collection":"pub.example.story", "rkey":"one"}]})).unwrap();
    let rss = output["rss"].as_str().unwrap();
    // RSS descriptions may be interpreted as HTML after an XML parser decodes entities.
    assert!(rss.contains("&amp;lt;img src=x onerror=evil()&amp;gt;"));
    assert!(rss.contains("&amp;lt;script&amp;gt;channel()"));
    assert!(!rss.contains('\0'));
}

#[test]
fn mixed_collection_requires_confirmation_before_any_export_or_publication() {
    let mut store = Store::memory();
    source(&mut store, "open", json!({"text":"Open"}), "public");
    source(
        &mut store,
        "private",
        json!({"text":"Chosen private"}),
        "cap",
    );
    let parent = tempfile::tempdir().unwrap();
    let directory = parent.path().join("share");
    let request =
        json!({"action":"export", "collection":"pub.example.story", "outputDir":directory});
    assert!(execute(&mut store, request.clone()).is_err());
    assert!(!directory.exists());
    let mut confirmed = request;
    confirmed["confirmPublish"] = json!(true);
    let output = execute(&mut store, confirmed).unwrap();
    assert!(output["html"].as_str().unwrap().contains("Chosen private"));
    assert_eq!(
        store.get("pub.example.story", "private").unwrap()["vis"],
        "cap"
    );
}

#[test]
fn private_publication_at_same_key_is_not_silently_upgraded() {
    let mut store = Store::memory();
    source(&mut store, "one", json!({"text":"Open"}), "public");
    let original = store
        .put(
            "pub.tangled.publication",
            "private",
            json!({"vendor.secret":"KEEP_PRIVATE"}),
            "cap",
        )
        .unwrap();
    let request = json!({"action":"publish", "confirmPublish":true, "key":"private", "collection":"pub.example.story"});
    assert!(execute(&mut store, request).is_err());
    assert_eq!(
        store.get("pub.tangled.publication", "private").unwrap(),
        original
    );
}
