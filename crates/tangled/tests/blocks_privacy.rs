mod common;

use common::Node;
use serde_json::json;
use tangled::{
    car,
    cid::Cid,
    http::{KnotRequest, KnotResponse, Method},
};

fn blocks(node: &Node, cids: &[String], authorization: Option<String>) -> KnotResponse {
    node.store
        .lock()
        .unwrap()
        .handle(&KnotRequest {
            method: Method::Post,
            path: "/xrpc/tangled.sync.getBlocks".into(),
            query: vec![],
            headers: authorization
                .map(|value| vec![("Authorization".into(), value)])
                .unwrap_or_default(),
            body: Some(serde_json::to_vec(&json!({"cids":cids})).unwrap()),
            content_type: Some("application/json".into()),
        })
        .unwrap()
}

fn has_block(response: &KnotResponse, cid: &str) -> bool {
    response.status == 200
        && response.content_type.contains("car")
        && car::decode(&response.body)
            .unwrap()
            .blocks
            .iter()
            .any(|(key, _)| key.to_string() == cid)
}

#[tokio::test]
async fn nonpublic_record_blocks_require_matching_capability() {
    let alice = Node::named("alice-block-privacy");
    let bob = Node::named("bob-block-privacy");
    for vis in ["cap", "sealed"] {
        let record = alice
            .publish(alice.unsigned(
                "app.tangled.import.object",
                vis,
                vis,
                json!({"alg":"hpke-record","enc":"00","ct":"private ciphertext"}),
            ))
            .await
            .unwrap();
        let cid = record.cid.to_string();
        assert_eq!(
            blocks(&alice, std::slice::from_ref(&cid), None).status,
            401,
            "{vis}"
        );
        let wrong = format!("tangled://{}/other.collection", alice.did());
        let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &wrong).unwrap();
        let header = bob.invocation_header(&cap, "repo/read", &wrong, &alice.did());
        assert_eq!(
            blocks(&alice, std::slice::from_ref(&cid), Some(header)).status,
            403
        );
        let covered = format!("tangled://{}/app.tangled.import.object/{vis}", alice.did());
        let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &covered).unwrap();
        let header = bob.invocation_header(&cap, "repo/read", &covered, &alice.did());
        assert!(has_block(
            &blocks(&alice, std::slice::from_ref(&cid), Some(header)),
            &cid
        ));
    }
}

#[tokio::test]
async fn private_blob_blocks_honor_repo_read_and_blob_get_capabilities() {
    let alice = Node::named("alice-blob-block-privacy");
    let bob = Node::named("bob-blob-block-privacy");
    let bytes = b"encrypted imported media".to_vec();
    let cid = Cid::raw(&bytes).to_string();
    {
        let mut engine = alice.store.lock().unwrap();
        // Archive chunks live in both maps; neither path may bypass auth.
        engine.blocks.insert(cid.clone(), bytes.clone());
        engine.blobs.insert((alice.did(), cid.clone()), bytes);
    }
    alice
        .publish(alice.unsigned(
            "app.tangled.import.file",
            "media",
            "sealed",
            json!({"alg":"hpke-record","enc":"00","ct":"ciphertext","blobs":[cid]}),
        ))
        .await
        .unwrap();
    assert_eq!(blocks(&alice, std::slice::from_ref(&cid), None).status, 401);
    for (command, resource) in [
        (
            "repo/read",
            format!("tangled://{}/app.tangled.import.file/media", alice.did()),
        ),
        ("blob/get", format!("tangled://{}/blob/{cid}", alice.did())),
    ] {
        let (cap, _) = alice.issue_cap(&bob.did(), command, &resource).unwrap();
        let header = bob.invocation_header(&cap, command, &resource, &alice.did());
        assert!(has_block(
            &blocks(&alice, std::slice::from_ref(&cid), Some(header)),
            &cid
        ));
    }
    let wrong = format!("tangled://{}/other", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &wrong).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &wrong, &alice.did());
    assert_eq!(
        blocks(&alice, std::slice::from_ref(&cid), Some(header)).status,
        403
    );
}

#[tokio::test]
async fn public_record_and_shared_blob_blocks_and_merkle_proofs_remain_public() {
    let alice = Node::named("alice-public-block-sharing");
    let bytes = b"shared public media".to_vec();
    let cid = Cid::raw(&bytes).to_string();
    {
        let mut engine = alice.store.lock().unwrap();
        engine.blocks.insert(cid.clone(), bytes.clone());
        engine.blobs.insert((alice.did(), cid.clone()), bytes);
    }
    alice
        .publish(alice.unsigned("private.media", "one", "sealed", json!({"blob":cid})))
        .await
        .unwrap();
    let public = alice
        .publish(alice.unsigned("pub.tangled.media", "two", "public", json!({"blob":cid})))
        .await
        .unwrap();
    let head = alice.store.lock().unwrap().heads[&alice.did()].clone();
    let wanted = vec![public.cid.to_string(), cid, head.commit, head.data];
    let response = blocks(&alice, &wanted, None);
    for cid in wanted {
        assert!(has_block(&response, &cid), "{cid}");
    }
}

#[tokio::test]
async fn blob_map_fallback_and_orphan_blobs_are_not_an_authentication_bypass() {
    let alice = Node::named("alice-orphan-block-privacy");
    let bytes = b"unreferenced private upload".to_vec();
    let cid = Cid::raw(&bytes).to_string();
    alice
        .store
        .lock()
        .unwrap()
        .blobs
        .insert((alice.did(), cid.clone()), bytes);
    assert_eq!(blocks(&alice, &[cid], None).status, 401);
}

#[tokio::test]
async fn circle_read_capability_authorizes_scoped_record_and_blob_blocks() {
    let alice = Node::named("alice-circle-block-privacy");
    let bob = Node::named("bob-circle-block-privacy");
    let bytes = b"encrypted circle media".to_vec();
    let cid = Cid::raw(&bytes).to_string();
    alice
        .store
        .lock()
        .unwrap()
        .blobs
        .insert((alice.did(), cid.clone()), bytes);
    let mut unsigned = alice.unsigned(
        "pub.tangled.chat",
        "message",
        "sealed",
        json!({"alg":"epoch-aead","epoch":0,"ct":"00","blobs":[cid]}),
    );
    unsigned["circle"] = json!(format!("{}#friends", alice.did()));
    let record = alice.publish(unsigned).await.unwrap();
    let resource = tangled::capability::circle_resource_uri(&alice.did(), "friends");
    let (cap, _) = alice
        .issue_cap(&bob.did(), "circle/read", &resource)
        .unwrap();
    let header = bob.invocation_header(&cap, "circle/read", &resource, &alice.did());
    let response = blocks(&alice, &[record.cid.to_string(), cid.clone()], Some(header));
    assert!(has_block(&response, &cid));
    assert!(has_block(&response, &record.cid.to_string()));
}
