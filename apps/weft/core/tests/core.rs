use std::sync::{Arc, Mutex};

use tangled::http::{KnotClient, PutRecordRequest};
use tangled::MockKnot;
use weft_core::{EdgeKind, Explorer, KnotUrl, LookupHit};

fn hosting(did: &str) -> MockKnot {
    let mut k = MockKnot::hosting(did);
    k.set_public_peers(&["https://b.example".into()]);
    k
}

#[tokio::test]
async fn local_stats_counts_public_records() {
    let did = "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs";
    let engine = Arc::new(Mutex::new(hosting(did)));
    let client = KnotClient::mock_shared(engine.clone());
    client
        .put_record(&PutRecordRequest {
            record: serde_json::json!({
                "v": 1,
                "collection": "pub.tangled.post",
                "rkey": "p1",
                "vis": "public",
                "body": { "text": "hi" }
            }),
        })
        .await
        .unwrap();

    let ex = Explorer::mock(engine);
    let s = ex.local_stats().await.unwrap();
    assert_eq!(s.did, did);
    assert_eq!(s.counts.records, 1);
    assert_eq!(s.counts.peers, 1);
}

#[tokio::test]
async fn assemble_edges_to_advertised_peers() {
    let did = "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs";
    let ex = Explorer::mock(Arc::new(Mutex::new(hosting(did))));
    let view = ex.assemble().await.unwrap();
    assert_eq!(view.home, "mock://");
    assert!(view.knots.iter().any(|n| n.url == "mock://" && n.reachable));
    assert!(view
        .knots
        .iter()
        .any(|n| n.url == "https://b.example" && !n.reachable));
    assert!(view.edges.iter().any(|e| {
        e.kind == EdgeKind::Peer && e.from == "mock://" && e.to == "https://b.example"
    }));
    assert!(view.totals.knots >= 2);
}

#[tokio::test]
async fn lookup_did_returns_identity() {
    let did = "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs";
    let engine = Arc::new(Mutex::new(hosting(did)));
    KnotClient::mock_shared(engine.clone())
        .put_record(&PutRecordRequest {
            record: serde_json::json!({
                "v": 1,
                "collection": "pub.tangled.post",
                "rkey": "p1",
                "vis": "public",
                "body": { "text": "hi" }
            }),
        })
        .await
        .unwrap();
    let ex = Explorer::mock(engine);
    match ex.lookup(did).await.unwrap() {
        LookupHit::Identity {
            did: got, records, ..
        } => {
            assert_eq!(got, did);
            assert_eq!(records, Some(1));
        }
        other => panic!("expected identity, got {other:?}"),
    }
}

#[tokio::test]
async fn http_constructor_rejects_empty_url() {
    assert!(Explorer::http(KnotUrl(String::new())).is_err());
}
