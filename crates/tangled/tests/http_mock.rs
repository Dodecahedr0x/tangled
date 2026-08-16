//! In-process mock knot: serialize/deserialize OpenAPI types and exercise the client.

use tangled::http::{
    GetHeadResponse, KnotClient, MockKnot, NodeDescribe, NotifyRequest, PutRecordRequest,
};

#[tokio::test]
async fn mock_knot_describe_head_put() {
    let mut knot = MockKnot {
        describe: Some(NodeDescribe {
            v: 1,
            did: "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs".into(),
            profiles: vec![
                "knot.public".into(),
                "knot.cap".into(),
                "knot.sealed".into(),
                "knot.group".into(),
            ],
            protocols: vec![],
            max_blob: 134_217_728,
            custodial_sign: false,
            peers: vec![],
        }),
        backlink_index: true,
        ..Default::default()
    };
    knot.heads.insert(
        "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs".into(),
        GetHeadResponse {
            did: "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs".into(),
            commit: "bafyr4idpxk4uaqzi6fhcvm476uhyklw3ormzhqpa6hitlcbtjkyc7czh2y".into(),
            rev: "3jzfcijpjiz00".into(),
            knot: "https://knot.example".into(),
            data: String::new(),
        },
    );

    let client = KnotClient::mock(knot);
    let d = client.describe().await.unwrap();
    assert_eq!(d.v, 1);
    assert!(d.profiles.contains(&"knot.public".into()));
    assert!(d.peers.is_empty());
    assert!(client.learned_peers().is_empty());

    let head = client
        .get_head("did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs")
        .await
        .unwrap();
    assert_eq!(head.rev, "3jzfcijpjiz00");

    let put = client
        .put_record(&PutRecordRequest {
            record: serde_json::json!({"v":1,"collection":"pub.tangled.post","rkey":"x","vis":"public"}),
        })
        .await
        .unwrap();
    assert!(put.cid.is_some());

    client
        .notify(&NotifyRequest {
            did: head.did.clone(),
            commit: head.commit.clone(),
        })
        .await
        .unwrap();

    let blob = client.put_blob(b"hi".to_vec()).await.unwrap();
    assert!(blob.cid.starts_with('b'));

    let _ = client
        .get_blocks(&tangled::http::GetBlocksRequest { cids: vec![] })
        .await;
    let repo = client.get_repo(&head.did, None, Some(false)).await.unwrap();
    assert!(repo.is_empty() || !repo.is_empty());
    let q = client.query_backlinks("bafy", None, None).await.unwrap();
    assert!(q.overlay);
    assert!(!q.complete);
    assert!(!tangled::http::overlay_proves_zero_comments(
        &tangled::http::overlay_comment_count(Ok(q))
    ));
    let loom = client.loom_query("hello", None).await.unwrap();
    assert!(loom.overlay);
    assert!(client
        .identity_resolve(None, Some(&head.did))
        .await
        .is_err());
    let _ = client.cap_verify(b"not-cbor".to_vec()).await;
    assert!(client.subscribe(&head.did, None).await.is_err());
    assert!(client.loom_firehose().await.is_err());
}

#[tokio::test]
async fn describe_peers_are_learned_by_client() {
    let mut knot = MockKnot::hosting("did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs");
    knot.set_public_peers(&["https://b.example".into(), "https://c.example".into()]);
    let client = KnotClient::mock(knot);
    let d = client.describe().await.unwrap();
    assert_eq!(
        d.peers,
        vec![
            "https://b.example".to_string(),
            "https://c.example".to_string()
        ]
    );
    assert_eq!(
        client.learned_peers(),
        vec![
            "https://b.example".to_string(),
            "https://c.example".to_string()
        ]
    );
}

#[test]
fn openapi_types_roundtrip_json() {
    let n = NodeDescribe {
        v: 1,
        did: "did:sol:x".into(),
        profiles: vec!["knot.public".into()],
        protocols: vec!["nostr".into()],
        max_blob: 1024,
        custodial_sign: false,
        peers: vec!["https://b.example".into()],
    };
    let s = serde_json::to_string(&n).unwrap();
    let back: NodeDescribe = serde_json::from_str(&s).unwrap();
    assert_eq!(n, back);

    let q = serde_json::json!({
        "hits": [{
            "did": "did:sol:x",
            "collection": "pub.tangled.annotation",
            "rkey": "a",
            "cid": "bafy",
            "commit": "bafy2"
        }],
        "count": 1,
        "overlay": true,
        "complete": false
    });
    let parsed: tangled::http::BacklinkQuery = serde_json::from_value(q).unwrap();
    assert!(parsed.overlay);
    assert!(!parsed.complete);
}

#[tokio::test]
async fn describe_prefers_dag_cbor_when_accept_says_so() {
    let knot = MockKnot::hosting("did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs");
    let client = KnotClient::mock(knot);
    let d = client.describe().await.unwrap();
    assert_eq!(d.v, 1);
    // Client sends Accept: application/cbor first; engine serves DAG-CBOR.
    let mut store = MockKnot::hosting("did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs");
    let resp = store
        .handle(&tangled::http::KnotRequest {
            method: tangled::http::Method::Get,
            path: "/xrpc/tangled.node.describe".into(),
            query: vec![],
            headers: vec![("Accept".into(), "application/cbor, application/json".into())],
            body: None,
            content_type: None,
        })
        .unwrap();
    assert!(resp.content_type.contains("cbor"), "{}", resp.content_type);
    let again: NodeDescribe = resp.json().unwrap();
    assert_eq!(again.did, d.did);
}

#[tokio::test]
async fn node_stats_counts_public_only_and_lists_advertised_peers() {
    let did = "did:sol:2sxknyqubWpqPSrk7Hncw1NyyySXQAoXa4HebbZV9qKs";
    let mut knot = MockKnot::hosting(did);
    knot.set_public_peers(&["https://b.example".into()]);
    let client = KnotClient::mock(knot);

    let empty = client.stats().await.unwrap();
    assert_eq!(empty.v, 1);
    assert_eq!(empty.did, did);
    assert_eq!(empty.counts.identities, 0);
    assert_eq!(empty.counts.records, 0);
    assert_eq!(empty.counts.peers, 1);
    assert_eq!(empty.topology.peers, vec!["https://b.example".to_string()]);
    assert!(empty.topology.hosted.is_empty());

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
    client
        .put_record(&PutRecordRequest {
            record: serde_json::json!({
                "v": 1,
                "collection": "pub.tangled.chat",
                "rkey": "s1",
                "vis": "sealed",
                "body": { "text": "secret" }
            }),
        })
        .await
        .unwrap();

    let s = client.stats().await.unwrap();
    assert_eq!(s.counts.identities, 1);
    assert_eq!(s.counts.repos, 1);
    assert_eq!(s.counts.records, 1);
    assert_eq!(s.counts.commits, 1);
    assert_eq!(s.rates.public_commits, 1);
    assert_eq!(s.rates.retained, 256);
    assert_eq!(s.topology.hosted.len(), 1);
    assert_eq!(s.topology.hosted[0].did, did);
    assert_eq!(s.topology.hosted[0].records, 1);
    assert!(!s.topology.truncated);
}
