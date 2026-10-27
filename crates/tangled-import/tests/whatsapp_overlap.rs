use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::{parse, ArchiveStore};

#[test]
fn ambiguous_overlapping_exports_preserve_candidates_instead_of_losing_messages() {
    let dir = tempfile::tempdir().unwrap();
    let id = Identity::from_seeds([103; 32], [104; 32]);
    let did = id.did_string();
    let mut engine = KnotEngine::hosting(&did);
    engine.register_identity(&id);
    let mut store = ArchiveStore::open(dir.path()).unwrap();
    let first=b"01/02/2024, 10:00 - Alex: Before\n01/02/2024, 10:01 - Alex: Same\n01/02/2024, 10:01 - Alex: Same\n";
    let second=b"01/02/2024, 10:01 - Alex: Same\n01/02/2024, 10:01 - Alex: Same\n01/02/2024, 10:02 - Alex: After\n";
    for bytes in [&first[..], &second[..], &second[..]] {
        let batch = parse("whatsapp", "my-account", "chat.txt", bytes).unwrap();
        store
            .ingest(&did, &id, &mut engine, "chat.txt", bytes, &batch)
            .unwrap();
    }
    let objects = store.objects(&did, &id).unwrap();
    assert!(
        objects
            .iter()
            .filter(|o| o.object.text.as_deref() == Some("Same"))
            .count()
            >= 3,
        "an uncertain overlap must not erase the third distinct message"
    );
    assert_eq!(
        store.receipts(&did, &id).unwrap().len(),
        2,
        "an exact export replay remains idempotent"
    );
}
