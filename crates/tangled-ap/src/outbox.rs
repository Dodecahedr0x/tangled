//! Public ActivityPub outbox paging (OrderedCollection → first page).
//!
//! Inbox POST verifies HTTP signatures. Outbox GET is public; this module
//! only parses JSON. The knot fetches the collection and first page, then
//! ingests Create/Note the same way as inbox.

use serde_json::Value;

use crate::activity::{parse_inbound, Inbound};
use crate::note::{as_id, type_name};

/// How to reach the first page of an `OrderedCollection`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FirstPage {
    Url(String),
    Embedded(Value),
    None,
}

/// Items plus paging links from a collection or page document.
#[derive(Clone, Debug, PartialEq)]
pub struct OutboxIndex {
    pub items: Vec<Value>,
    pub first: FirstPage,
    pub next: Option<String>,
}

pub fn parse_outbox_collection(doc: &Value) -> OutboxIndex {
    let items = ordered_items(doc);
    let first = if items.is_empty() {
        first_page_of(doc)
    } else {
        FirstPage::None
    };
    OutboxIndex {
        items,
        first,
        next: None,
    }
}

pub fn parse_outbox_page(doc: &Value) -> OutboxIndex {
    OutboxIndex {
        items: ordered_items(doc),
        first: FirstPage::None,
        next: as_id(doc.get("next")),
    }
}

/// Create/Note only. Announce, Undo, and bare IRIs are skipped.
pub fn inbound_from_outbox_item(item: &Value) -> Option<Inbound> {
    match type_name(item).as_str() {
        "Create" => parse_inbound(item).ok(),
        "Note" => {
            let actor = as_id(item.get("attributedTo")).unwrap_or_default();
            Some(Inbound::CreateNote {
                activity_id: as_id(Some(item)).unwrap_or_default(),
                actor,
                note: item.clone(),
            })
        }
        _ => None,
    }
}

pub fn first_page_of(doc: &Value) -> FirstPage {
    match doc.get("first") {
        Some(Value::String(url)) if !url.is_empty() => FirstPage::Url(url.clone()),
        Some(obj) if obj.is_object() => FirstPage::Embedded(obj.clone()),
        _ => FirstPage::None,
    }
}

fn ordered_items(doc: &Value) -> Vec<Value> {
    match doc.get("orderedItems").or_else(|| doc.get("items")) {
        Some(Value::Array(a)) => a.clone(),
        Some(other) => vec![other.clone()],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn collection_fixture_points_at_first_page_url() {
        let doc = fixture("outbox-collection.json");
        let idx = parse_outbox_collection(&doc);
        assert!(
            idx.items.is_empty(),
            "Mastodon collections are empty shells"
        );
        assert_eq!(
            idx.first,
            FirstPage::Url("https://ex.example/users/alice/outbox?page=true".into())
        );
        assert_eq!(idx.next, None);
    }

    #[test]
    fn page_fixture_yields_create_and_note_skips_announce_and_iri() {
        let doc = fixture("outbox-page.json");
        let idx = parse_outbox_page(&doc);
        assert_eq!(idx.items.len(), 4);
        assert_eq!(
            idx.next.as_deref(),
            Some("https://ex.example/users/alice/outbox?max_id=2")
        );

        let inbound: Vec<Inbound> = idx
            .items
            .iter()
            .filter_map(inbound_from_outbox_item)
            .collect();
        assert_eq!(
            inbound.len(),
            2,
            "Create + bare Note; skip Announce and IRI"
        );
        match &inbound[0] {
            Inbound::CreateNote { note, actor, .. } => {
                assert_eq!(actor, "https://ex.example/users/alice");
                assert_eq!(note["content"], "third toot");
            }
            other => panic!("{other:?}"),
        }
        match &inbound[1] {
            Inbound::CreateNote { note, actor, .. } => {
                assert_eq!(actor, "https://ex.example/users/alice");
                assert_eq!(note["content"], "bare note on the page");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn embedded_first_page_needs_no_fetch() {
        let doc = serde_json::json!({
            "type": "OrderedCollection",
            "first": {
                "type": "OrderedCollectionPage",
                "orderedItems": [{
                    "type": "Create",
                    "actor": "https://ex.example/users/a",
                    "object": {
                        "type": "Note",
                        "id": "https://ex.example/users/a/statuses/1",
                        "content": "inline"
                    }
                }]
            }
        });
        match first_page_of(&doc) {
            FirstPage::Embedded(page) => {
                let idx = parse_outbox_page(&page);
                let notes: Vec<_> = idx
                    .items
                    .iter()
                    .filter_map(inbound_from_outbox_item)
                    .collect();
                assert_eq!(notes.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn inline_ordered_items_on_collection() {
        let doc = serde_json::json!({
            "type": "OrderedCollection",
            "orderedItems": [{
                "type": "Note",
                "id": "https://ex.example/n/1",
                "attributedTo": "https://ex.example/users/a",
                "content": "direct"
            }]
        });
        let idx = parse_outbox_collection(&doc);
        assert_eq!(idx.items.len(), 1);
        assert_eq!(idx.first, FirstPage::None);
        let got = inbound_from_outbox_item(&idx.items[0]).unwrap();
        match got {
            Inbound::CreateNote { note, .. } => assert_eq!(note["content"], "direct"),
            other => panic!("{other:?}"),
        }
    }
}
