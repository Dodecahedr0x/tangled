mod common;
use common::Node;
use serde_json::json;
use tangled::{
    car,
    http::{KnotRequest, KnotResponse, Method},
};
fn export(node: &Node, did: &str, authorization: String) -> KnotResponse {
    node.store
        .lock()
        .unwrap()
        .handle(&KnotRequest {
            method: Method::Post,
            path: "/xrpc/tangled.sync.getRepoAuth".into(),
            query: vec![],
            headers: vec![("Authorization".into(), authorization)],
            body: Some(serde_json::to_vec(&json!({"did":did,"blobs":true})).unwrap()),
            content_type: Some("application/json".into()),
        })
        .unwrap()
}
#[tokio::test]
async fn legacy_circle_export_excludes_other_circle_and_collection_payloads() {
    let node = Node::named("research-sync-circle");
    let reader = Node::named("research-sync-circle-reader");
    let mut permitted = vec![];
    let mut forbidden = vec![];
    for name in ["friends", "strangers"] {
        let log = node
            .publish(node.unsigned("sys.circle", name, "cap", json!({"v":1,"epoch":0})))
            .await
            .unwrap();
        let mut value = node.unsigned("pub.tangled.chat", name, "sealed", json!({"ct":name}));
        value["circle"] = json!(format!("{}#{name}", node.did()));
        let payload = node.publish(value).await.unwrap();
        if name == "friends" {
            permitted.extend([log.cid, payload.cid]);
        } else {
            forbidden.extend([log.cid, payload.cid]);
        }
    }
    forbidden.push(
        node.publish(node.unsigned(
            "private.collection",
            "one",
            "cap",
            json!({"text":"private"}),
        ))
        .await
        .unwrap()
        .cid,
    );
    let resource = tangled::capability::circle_resource_uri(&node.did(), "friends");
    let (cap, _) = node
        .issue_cap(&reader.did(), "circle/read", &resource)
        .unwrap();
    let header = reader.invocation_header(&cap, "circle/read", &resource, &node.did());
    let response = export(&node, &node.did(), header);
    assert_eq!(response.status, 200);
    let archive = car::decode(&response.body).unwrap();
    for cid in permitted {
        assert!(archive.get(&cid).is_some());
    }
    for cid in forbidden {
        assert!(archive.get(&cid).is_none(), "unauthorized {cid}");
    }
}

#[tokio::test]
async fn legacy_export_capability_does_not_widen_exact_record_scope() {
    let node = Node::named("research-sync-scopes");
    let reader = Node::named("research-sync-scope-reader");
    let first = node
        .publish(node.unsigned("private.collection", "one", "cap", json!({"text":"first"})))
        .await
        .unwrap();
    let second = node
        .publish(node.unsigned("private.collection", "two", "cap", json!({"text":"second"})))
        .await
        .unwrap();
    let resource = format!("tangled://{}/private.collection/one", node.did());
    let (cap, _) = node
        .issue_cap(&reader.did(), "repo/read", &resource)
        .unwrap();
    let header = reader.invocation_header(&cap, "repo/read", &resource, &node.did());
    let response = export(&node, &node.did(), header.clone());
    let archive = car::decode(&response.body).unwrap();
    assert!(archive.get(&first.cid).is_some());
    assert!(
        archive.get(&second.cid).is_none(),
        "exact record cap widened"
    );
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn legacy_export_rejects_foreign_did_before_consuming_one_use_capability() {
    let node = Node::named("scope-owner");
    let other = Node::named("scope-other-owner");
    let reader = Node::named("scope-reader");
    let resource = format!("tangled://{}/private.collection/one", node.did());
    let (mut cap, _) = node
        .issue_cap(&reader.did(), "repo/read", &resource)
        .unwrap();
    cap.pol.push(json!(["max_uses", 1]));
    cap.sign(node.identity.signing_key()).unwrap();
    let header = reader.invocation_header(&cap, "repo/read", &resource, &node.did());
    node.publish(node.unsigned("private.collection", "one", "cap", json!({"text":"owner"})))
        .await
        .unwrap();
    let foreign = other
        .sign(&other.unsigned(
            "private.collection",
            "one",
            "cap",
            json!({"text":"foreign"}),
        ))
        .unwrap();
    node.store
        .lock()
        .unwrap()
        .ingest_record(foreign.json)
        .unwrap();
    let response = export(&node, &other.did(), header.clone());
    assert_eq!(response.status, 403, "foreign DID must be rejected");
    assert_eq!(
        node.store
            .lock()
            .unwrap()
            .cap_uses
            .get(&cap.cid().unwrap().to_string()),
        None,
        "scope rejection must not consume the capability"
    );
    assert_eq!(
        export(&node, &node.did(), header).status,
        200,
        "one allowed use must remain after rejecting foreign DID"
    );
}
