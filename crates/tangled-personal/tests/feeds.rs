use serde_json::{json, Value};
use tangled_personal::{feeds::execute, Store};

const URL: &str = "https://example.org/feed.xml";
const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/"><channel><title>Example</title><item><guid>source-1</guid><title>A &amp; B</title><link>https://example.org/one</link><content:encoded><![CDATA[<p>Hello</p>]]></content:encoded><pubDate>Tue, 27 Oct 2026 10:00:00 GMT</pubDate></item><item><guid>source-2</guid><title>Second</title><description>More</description></item></channel></rss>"#;
fn subscribed(store: &mut Store) -> Value {
    execute(
        store,
        json!({"action":"subscribe","url":URL,"title":"Example & friends","tags":["rust"]}),
    )
    .unwrap()
}
fn ingest(store: &mut Store, xml: &str) -> Value {
    execute(store, json!({"action":"ingest","url":URL,"xml":xml})).unwrap()
}

#[test]
fn subscriptions_are_private_idempotent_and_export_escaped_opml() {
    let mut store = Store::memory();
    let first = subscribed(&mut store);
    let again = subscribed(&mut store);
    assert_eq!(first["cid"], again["cid"]);
    assert_eq!(first["vis"], "cap");
    let list = execute(&mut store, json!({"action":"list"})).unwrap();
    assert_eq!(list["subscriptions"].as_array().unwrap().len(), 1);
    let opml = execute(&mut store, json!({"action":"opml"})).unwrap();
    assert!(opml["opml"]
        .as_str()
        .unwrap()
        .contains("Example &amp; friends"));
    execute(&mut store, json!({"action":"unsubscribe","url":URL})).unwrap();
    assert!(
        execute(&mut store, json!({"action":"list"})).unwrap()["subscriptions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rss_ingest_preserves_evidence_and_deduplicates_source_ids() {
    let mut store = Store::memory();
    subscribed(&mut store);
    assert_eq!(ingest(&mut store, RSS)["imported"], 2);
    let before = store.list(Some("pub.tangled.feedItem")).unwrap();
    assert_eq!(ingest(&mut store, RSS)["imported"], 0);
    assert_eq!(before, store.list(Some("pub.tangled.feedItem")).unwrap());
    let item = before
        .iter()
        .find(|i| i["body"]["sourceId"] == "source-1")
        .unwrap();
    assert_eq!(item["vis"], "cap");
    assert_eq!(item["body"]["title"], "A & B");
    assert_eq!(item["body"]["body"], "<p>Hello</p>");
    assert_eq!(item["body"]["link"], "https://example.org/one");
    assert_eq!(item["body"]["publishedAt"], "Tue, 27 Oct 2026 10:00:00 GMT");
    let blob = item["body"]["provenance"]["blob"].as_str().unwrap();
    assert_eq!(store.get_blob(blob).unwrap(), RSS.as_bytes());
    assert!(item["body"]["subscription"]
        .as_str()
        .unwrap()
        .starts_with("tangled://"));
}

#[test]
fn atom_ingest_handles_empty_links_cdata_and_xhtml_as_inert_text() {
    let mut store = Store::memory();
    subscribed(&mut store);
    let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><id>tag:example.org,2026:a</id><title>Atom</title><link rel="self" href="https://example.org/api"/><link href="https://example.org/a?x=1&amp;y=2"/><content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml"><p>Safe <b>words</b></p></div></content><updated>2026-10-27T10:00:00Z</updated></entry></feed>"#;
    assert_eq!(ingest(&mut store, xml)["imported"], 1);
    let item = store.list(Some("pub.tangled.feedItem")).unwrap().remove(0);
    assert_eq!(item["body"]["sourceId"], "tag:example.org,2026:a");
    assert_eq!(item["body"]["link"], "https://example.org/a?x=1&y=2");
    assert_eq!(item["body"]["body"], "Safe words");
    assert_eq!(item["body"]["bodyType"], "text");
}

#[test]
fn reading_state_filters_digest_and_extensions_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let item;
    {
        let mut store = Store::open(dir.path()).unwrap();
        subscribed(&mut store);
        ingest(&mut store, RSS);
        let queue = execute(&mut store, json!({"action":"queue"})).unwrap();
        assert_eq!(queue["items"].as_array().unwrap().len(), 2);
        item = queue["items"][0]["uri"].as_str().unwrap().to_string();
        let mark = execute(
            &mut store,
            json!({"action":"mark","item":item,"read":true,"starred":true,"tags":["later"]}),
        )
        .unwrap();
        let mut body = mark["body"].clone();
        body["thirdParty"] = json!({"keep":true});
        store
            .put(
                "pub.tangled.readingState",
                mark["rkey"].as_str().unwrap(),
                body,
                "cap",
            )
            .unwrap();
        execute(
            &mut store,
            json!({"action":"mark","item":item,"read":false}),
        )
        .unwrap();
    }
    let mut store = Store::open(dir.path()).unwrap();
    let queue = execute(
        &mut store,
        json!({"action":"queue","starred":true,"tags":["later"]}),
    )
    .unwrap();
    assert_eq!(queue["items"].as_array().unwrap().len(), 1);
    assert_eq!(queue["items"][0]["uri"], item);
    let states = store.list(Some("pub.tangled.readingState")).unwrap();
    assert_eq!(states[0]["body"]["thirdParty"]["keep"], true);
    let digest = execute(&mut store, json!({"action":"digest","starred":true})).unwrap();
    assert_eq!(digest["count"], 1);
    assert!(!digest["text"].as_str().unwrap().is_empty());
}

#[test]
fn invalid_xml_and_unsafe_sources_never_write_items() {
    let mut store = Store::memory();
    subscribed(&mut store);
    for xml in [
        "<rss><channel><item><title>Bad</title></item></channel>",
        "<html/>",
        "<!DOCTYPE rss [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><rss><channel/></rss>",
        "<rss/><rss/>",
        "<feed><entry><title>&unknown;</title></entry></feed>",
    ] {
        assert!(
            execute(&mut store, json!({"action":"ingest","url":URL,"xml":xml})).is_err(),
            "{xml}"
        );
    }
    let large = "x".repeat(2 * 1024 * 1024 + 1);
    assert!(execute(&mut store, json!({"action":"ingest","url":URL,"xml":large})).is_err());
    for url in [
        "file:///etc/passwd",
        "https://user:pass@example.org/rss",
        "javascript:alert(1)",
        "https://",
        "https://example.org/#fragment",
    ] {
        assert!(execute(&mut store, json!({"action":"subscribe","url":url})).is_err());
    }
    assert!(execute(
        &mut store,
        json!({"action":"mark","item":"tangled://missing","read":true})
    )
    .is_err());
    assert!(store.list(Some("pub.tangled.feedItem")).unwrap().is_empty());
    assert!(execute(&mut store, json!({"action":"queue","unread":"yes"})).is_err());
}

#[test]
fn structural_limits_and_oversized_items_reject_before_any_item_write() {
    let mut store = Store::memory();
    subscribed(&mut store);
    let deep = format!(
        "<feed>{}<entry><id>x</id></entry>{}</feed>",
        "<a>".repeat(64),
        "</a>".repeat(64)
    );
    let many = format!("<feed>{}</feed>", "<entry><id>x</id></entry>".repeat(1001));
    let body = format!(
        "<feed><entry><id>x</id><content>{}</content></entry></feed>",
        "x".repeat(256 * 1024 + 1)
    );
    // Individually bounded nested values must not amplify the retained XML tree without bound.
    let amplified = format!(
        "<feed>{}{}{}</feed>",
        "<a>".repeat(50),
        "x".repeat(256 * 1024),
        "</a>".repeat(50)
    );
    for xml in [deep, many, body, amplified] {
        assert!(execute(&mut store, json!({"action":"ingest","url":URL,"xml":xml})).is_err());
    }
    assert!(store.list(Some("pub.tangled.feedItem")).unwrap().is_empty());
}

#[test]
fn source_identity_is_scoped_to_subscription_and_fallback_link_is_stable() {
    let mut store = Store::memory();
    subscribed(&mut store);
    let second = "https://example.org/other.xml";
    execute(&mut store, json!({"action":"subscribe","url":second})).unwrap();
    ingest(&mut store, RSS);
    execute(
        &mut store,
        json!({"action":"ingest","url":second,"xml":RSS}),
    )
    .unwrap();
    assert_eq!(store.list(Some("pub.tangled.feedItem")).unwrap().len(), 4);
    let xml="<rss><channel><item><title>Linked</title><link>https://example.org/linked</link></item></channel></rss>";
    assert_eq!(ingest(&mut store, xml)["imported"], 1);
    assert_eq!(
        ingest(&mut store, &xml.replace("Linked", "Changed"))["imported"],
        0
    );
    let queue = execute(
        &mut store,
        json!({"action":"queue","url":URL,"tags":["rust"]}),
    )
    .unwrap();
    assert_eq!(queue["items"].as_array().unwrap().len(), 3);
}

#[test]
fn opml_replaces_xml_invalid_title_characters_and_bounds_canonical_urls() {
    let mut store = Store::memory();
    execute(
        &mut store,
        json!({"action":"subscribe","url":URL,"title":"Before\u{0000}After"}),
    )
    .unwrap();
    let output = execute(&mut store, json!({"action":"opml"})).unwrap();
    let opml = output["opml"].as_str().unwrap();
    assert!(!opml.contains('\0'));
    assert!(opml.contains("Before\u{fffd}After"));
    let long = format!("https://example.org/{}", "é".repeat(3000));
    assert!(execute(&mut store, json!({"action":"subscribe","url":long})).is_err());
    assert_eq!(
        store
            .list(Some("pub.tangled.feedSubscription"))
            .unwrap()
            .len(),
        1
    );
}
