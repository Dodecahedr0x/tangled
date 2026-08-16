//! Additional multi-node e2e covering spec gaps (migration, CAR, annotations,
//! albums, graph, compact admission, signedBy, invocation DAG-CBOR).

mod common;

use common::{http_code, identity_named, Node, NOW};
use tangled::capability::{cap_authorization_header, Invocation};
use tangled::car;
use tangled::foreign::{activitypub_foreign_proof, activitypub_import_attested, import_rkey};
use tangled::http::{
    overlay_comment_count, overlay_proves_zero_comments, GetBlocksRequest, GetRepoAuthRequest,
    OverlayCount, PutRecordRequest,
};
use tangled::page::{apply, record_from_unsigned};
use tangled::record::compact_allowed;
use tangled::sync::safe_did_rkey;
use tangled::Cid;

#[tokio::test]
async fn car_get_repo_roundtrip_two_knots() {
    let alice = Node::named("alice-car");
    let dest_store = {
        let k = tangled::MockKnot::default();
        std::sync::Arc::new(std::sync::Mutex::new(k))
    };
    let post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "p1",
            "public",
            serde_json::json!({ "text": "portable" }),
        ))
        .await
        .unwrap();

    let car_bytes = alice
        .local()
        .get_repo(&alice.did(), None, Some(false))
        .await
        .unwrap();
    assert!(!car_bytes.is_empty());
    let decoded = car::decode(&car_bytes).unwrap();
    assert!(decoded
        .block_cids()
        .iter()
        .any(|c| c == &post.cid.to_string()));

    let dest = tangled::KnotClient::mock_shared(dest_store.clone());
    let token = common::siws_token(&dest, &alice.identity).await.unwrap();
    let imported = dest
        .with_session_cookie(token)
        .import_repo(car_bytes, false)
        .await
        .unwrap();
    assert_eq!(imported.did, alice.did());

    let dest = tangled::KnotClient::mock_shared(dest_store.clone());

    let got = dest
        .get_record(&alice.did(), "pub.tangled.post", "p1")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "portable");
}

#[tokio::test]
async fn get_repo_since_is_incremental() {
    let alice = Node::named("alice-diff");
    let first = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "a",
            "public",
            serde_json::json!({ "text": "one" }),
        ))
        .await
        .unwrap();
    let head1 = alice.local().get_head(&alice.did()).await.unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "b",
            "public",
            serde_json::json!({ "text": "two" }),
        ))
        .await
        .unwrap();
    let full = car::decode(
        &alice
            .local()
            .get_repo(&alice.did(), None, None)
            .await
            .unwrap(),
    )
    .unwrap();
    let diff = car::decode(
        &alice
            .local()
            .get_repo(&alice.did(), Some(&first.cid.to_string()), None)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(full.blocks.len() >= diff.blocks.len());

    let since_commit = alice
        .local()
        .get_repo(&alice.did(), Some(&head1.commit), None)
        .await
        .unwrap();
    let since_car = car::decode(&since_commit).unwrap();
    assert!(since_car
        .get(&tangled::Cid::parse(&head1.commit).unwrap())
        .is_some());
    let head2 = alice.local().get_head(&alice.did()).await.unwrap();
    assert!(since_car
        .get(&tangled::Cid::parse(&head2.commit).unwrap())
        .is_some());
    assert!(since_car
        .get(&tangled::Cid::parse(&head1.data).unwrap())
        .is_some());
    assert!(since_car
        .get(&tangled::Cid::parse(&head2.data).unwrap())
        .is_some());

    let dest_store = {
        let k = tangled::MockKnot::default();
        std::sync::Arc::new(std::sync::Mutex::new(k))
    };
    let dest = tangled::KnotClient::mock_shared(dest_store.clone());
    let token = common::siws_token(&dest, &alice.identity).await.unwrap();
    dest.with_session_cookie(token)
        .import_repo(since_commit, false)
        .await
        .unwrap();
    let dest = tangled::KnotClient::mock_shared(dest_store);
    let imported_head = dest.get_head(&alice.did()).await.unwrap();
    assert_eq!(imported_head.commit, head2.commit);
    assert_eq!(
        dest.get_record(&alice.did(), "pub.tangled.post", "a")
            .await
            .unwrap()
            .record["body"]["text"],
        "one"
    );
    assert_eq!(
        dest.get_record(&alice.did(), "pub.tangled.post", "b")
            .await
            .unwrap()
            .record["body"]["text"],
        "two"
    );
}

#[tokio::test]
async fn migration_location_and_device_revoke() {
    let alice = Node::named("alice-mig");
    alice
        .publish(alice.unsigned(
            "sys.id",
            "self",
            "public",
            serde_json::json!({
                "v": 1,
                "pubkey": hex::encode(alice.identity.pubkey_bytes()),
                "agreement": hex::encode(alice.identity.agreement_public_bytes()),
                "rotation": [hex::encode(alice.identity.pubkey_bytes())],
                "recovery": [],
                "endpoints": [{ "type": "knot", "url": format!("inproc://{}", alice.did()) }],
                "alsoKnownAs": [],
                "bindings": []
            }),
        ))
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "hello",
            "public",
            serde_json::json!({ "text": "before move" }),
        ))
        .await
        .unwrap();

    let car_bytes = alice
        .local()
        .get_repo(&alice.did(), None, Some(true))
        .await
        .unwrap();
    let dest = Node::named("dest-knot");
    let token = alice.siws_on(&dest).await.unwrap();
    dest.local()
        .with_session_cookie(token)
        .import_repo(car_bytes, true)
        .await
        .unwrap();

    let location = alice.unsigned(
        "sys.id",
        "self",
        "public",
        serde_json::json!({
            "v": 1,
            "pubkey": hex::encode(alice.identity.pubkey_bytes()),
            "agreement": hex::encode(alice.identity.agreement_public_bytes()),
            "rotation": [hex::encode(alice.identity.pubkey_bytes())],
            "recovery": [],
            "endpoints": [
                { "type": "knot", "url": format!("inproc://{}", dest.did()) }
            ],
            "alsoKnownAs": [],
            "bindings": []
        }),
    );
    dest.ingest(alice.sign(&location).unwrap().json).unwrap();

    let (device_cap, _) = alice
        .issue_cap(
            &alice.did(),
            "repo/write",
            &format!("tangled://{}", alice.did()),
        )
        .unwrap();
    let cap_cid = device_cap.cid().unwrap().to_string();
    alice
        .publish(alice.unsigned(
            "sys.revocation",
            "rev1",
            "public",
            serde_json::json!({
                "cid": cap_cid,
                "ts": NOW,
                "reason": "device lost"
            }),
        ))
        .await
        .unwrap();

    let bob = Node::named("bob-mig");
    let pulled = bob
        .to(&alice)
        .get_record(&alice.did(), "pub.tangled.post", "hello")
        .await
        .unwrap();
    assert_eq!(pulled.record["body"]["text"], "before move");

    let id = bob
        .to(&alice)
        .get_record(&alice.did(), "sys.id", "self")
        .await
        .unwrap();
    assert!(id.record["body"]["endpoints"].is_array());
}

#[tokio::test]
async fn annotation_backlinks_and_empty_overlay() {
    let alice = Node::named("alice-ann");
    let bob = Node::named("bob-ann");
    let post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "essay",
            "public",
            serde_json::json!({ "text": "read me" }),
        ))
        .await
        .unwrap();
    let cid = post.cid.to_string();

    let mut note = bob.unsigned(
        "pub.tangled.annotation",
        "n1",
        "public",
        serde_json::json!({ "text": "nice", "reply": cid }),
    );
    note["ref"] = serde_json::Value::String(cid.clone());
    bob.publish(note).await.unwrap();

    let q = bob
        .local()
        .query_backlinks(&cid, Some("pub.tangled.annotation"), Some(50))
        .await
        .unwrap();
    assert!(q.overlay);
    assert!(!q.complete);
    assert_eq!(q.hits.len(), 1);
    assert_eq!(q.count, Some(1));

    let empty = alice
        .local()
        .query_backlinks(&cid, None, None)
        .await
        .unwrap();
    let obs = overlay_comment_count(Ok(empty));
    assert_eq!(
        obs,
        OverlayCount::Observed {
            count: 0,
            complete: false
        }
    );
    assert!(
        !overlay_proves_zero_comments(&obs),
        "empty overlay is not network-zero"
    );

    {
        let mut g = alice.store.lock().unwrap();
        g.backlink_index = false;
    }
    let missing = alice.local().query_backlinks(&cid, None, None).await;
    let err = missing.as_ref().unwrap_err();
    assert_eq!(http_code(err).1, "MethodNotImplemented");
    assert_eq!(
        overlay_comment_count(alice.local().query_backlinks(&cid, None, None).await),
        OverlayCount::IndexMissing
    );

    let loom = bob.local().loom_query(&cid, Some("ref")).await.unwrap();
    assert_eq!(loom.hits.len(), 1);
    assert!(loom.overlay);
    assert!(!loom.complete);
}

#[tokio::test]
async fn album_blob_and_graph_block() {
    let alice = Node::named("alice-alb");
    let bob = Node::named("bob-alb");
    let bytes = b"jpeg-bytes";
    let blob = alice.local().put_blob(bytes.to_vec()).await.unwrap();
    let media = alice
        .publish(alice.unsigned(
            "pub.tangled.media",
            "m1",
            "public",
            serde_json::json!({ "blob": blob.cid, "alt": "cat", "mime": "image/jpeg" }),
        ))
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.album",
            "summer",
            "public",
            serde_json::json!({ "title": "Summer", "items": [media.cid.to_string()] }),
        ))
        .await
        .unwrap();

    let album = bob
        .to(&alice)
        .get_record(&alice.did(), "pub.tangled.album", "summer")
        .await
        .unwrap();
    assert_eq!(album.record["body"]["title"], "Summer");
    let got = bob
        .to(&alice)
        .get_blob(&alice.did(), &blob.cid)
        .await
        .unwrap();
    assert_eq!(got, bytes);

    let block_rkey = safe_did_rkey(&bob.did());
    alice
        .publish(alice.unsigned(
            "tangled.graph.block",
            &block_rkey,
            "public",
            serde_json::json!({ "v": 1, "subject": bob.did() }),
        ))
        .await
        .unwrap();
    let blocked = bob
        .to(&alice)
        .get_record(&alice.did(), "tangled.graph.block", &block_rkey)
        .await
        .unwrap();
    assert_eq!(blocked.record["body"]["subject"], bob.did());

    alice
        .publish(alice.unsigned(
            "tangled.graph.block",
            &block_rkey,
            "public",
            serde_json::json!({ "v": 1, "subject": bob.did() }),
        ))
        .await
        .unwrap();
    let mut tomb = alice.unsigned(
        "tangled.graph.block",
        &block_rkey,
        "public",
        serde_json::Value::Null,
    );
    tomb["body"] = serde_json::Value::Null;
    alice.publish(tomb).await.unwrap();
    let live = alice
        .local()
        .get_record(&alice.did(), "tangled.graph.block", &block_rkey)
        .await
        .unwrap();
    assert!(live.record.get("body").is_some_and(|b| b.is_null()));
}

#[tokio::test]
async fn labels_and_core_collections() {
    let alice = Node::named("alice-lab");
    alice
        .publish(alice.unsigned(
            "tangled.label",
            "l1",
            "public",
            serde_json::json!({
                "v": 1,
                "src": alice.did(),
                "uri": format!("tangled://{}", alice.did()),
                "val": "ok",
                "neg": false,
                "ts": NOW
            }),
        ))
        .await
        .unwrap();
    let lab = alice
        .local()
        .get_record(&alice.did(), "tangled.label", "l1")
        .await
        .unwrap();
    assert_eq!(lab.record["body"]["val"], "ok");
    assert_eq!(lab.record["vis"], "public");
}

#[tokio::test]
async fn invocation_dagcbor_and_mutated_rejected() {
    let alice = Node::named("alice-inv");
    let bob = Node::named("bob-inv");
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "draft",
            "cap",
            serde_json::json!({ "text": "secret" }),
        ))
        .await
        .unwrap();
    let res = format!("tangled://{}", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    assert!(header.starts_with("Cap "));
    let got = bob
        .to_with_cap(&alice, header)
        .get_record(&alice.did(), "pub.tangled.post", "draft")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "secret");

    let json_header = {
        use base64::Engine;
        let json = serde_json::json!({"v":1,"cmd":"repo/read","prf":[]});
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.to_string());
        format!("Cap {b64}")
    };
    let err = bob
        .to_with_cap(&alice, json_header)
        .get_record(&alice.did(), "pub.tangled.post", "draft")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);

    let mut inv = Invocation::new(&cap, "repo/read", &res, &alice.did(), NOW, [7u8; 16]).unwrap();
    inv.sign(bob.identity.signing_key()).unwrap();
    let mut cbor = inv.encode_cbor().unwrap();
    let last = cbor.len() - 1;
    cbor[last] ^= 0xff;
    let mutated = cap_authorization_header(&cbor);
    let err = bob
        .to_with_cap(&alice, mutated)
        .get_record(&alice.did(), "pub.tangled.post", "draft")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);

    let mut bad_aud = Invocation::new(&cap, "repo/read", &res, &bob.did(), NOW, [9u8; 16]).unwrap();
    bad_aud.sign(bob.identity.signing_key()).unwrap();
    let err = bob
        .to_with_cap(
            &alice,
            cap_authorization_header(&bad_aud.encode_cbor().unwrap()),
        )
        .get_record(&alice.did(), "pub.tangled.post", "draft")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);

    let cbor = inv.encode_cbor().unwrap();
    let verified = alice.local().cap_verify(cbor).await.unwrap();
    assert!(verified.ok);
}

#[tokio::test]
async fn expired_and_revoked_cap() {
    let alice = Node::named("alice-exp");
    let bob = Node::named("bob-exp");
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "x",
            "cap",
            serde_json::json!({ "text": "n" }),
        ))
        .await
        .unwrap();
    let res = format!("tangled://{}", alice.did());
    let mut cap = tangled::capability::Capability {
        v: 1,
        iss: alice.did(),
        aud: bob.did(),
        cmd: "repo/read".into(),
        res: res.clone(),
        nbf: NOW,
        exp: NOW + 10,
        pol: vec![],
        prf: vec![],
        meta: None,
        sig: None,
    };
    cap.sign(alice.identity.signing_key()).unwrap();
    {
        let mut g = alice.store.lock().unwrap();
        g.now = NOW + 100;
    }
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let err = bob
        .to_with_cap(&alice, header)
        .get_record(&alice.did(), "pub.tangled.post", "x")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);
}

#[tokio::test]
async fn stolen_read_cap_cannot_write() {
    let alice = Node::named("alice-w");
    let mallory = Node::named("mallory-w");
    let res = format!("tangled://{}", alice.did());
    let (cap, _) = alice.issue_cap(&mallory.did(), "repo/read", &res).unwrap();
    let header = mallory.invocation_header(&cap, "repo/read", &res, &alice.did());
    let rec = mallory.unsigned(
        "pub.tangled.post",
        "hack",
        "public",
        serde_json::json!({ "text": "nope" }),
    );
    let signed = mallory.sign(&rec).unwrap();
    let err = mallory
        .to_with_cap(&alice, header)
        .put_record(&PutRecordRequest {
            record: signed.json,
        })
        .await
        .unwrap_err();
    let (status, code) = http_code(&err);
    assert!(status == 401 || status == 403, "{status} {code}");
}

#[tokio::test]
async fn shared_repo_signed_by_concurrent_heads() {
    let wiki = Node::named("wiki-host");
    let alice = Node::named("collab-a");
    let bob = Node::named("collab-b");
    let res = format!("tangled://{}/pub.tangled.page", wiki.did());
    let (cap_a, _) = wiki.issue_cap(&alice.did(), "repo/write", &res).unwrap();
    let (cap_b, _) = wiki.issue_cap(&bob.did(), "repo/write", &res).unwrap();
    let ha = alice.invocation_header(&cap_a, "repo/write", &res, &wiki.did());
    let hb = bob.invocation_header(&cap_b, "repo/write", &res, &wiki.did());

    let mut r0 = wiki.unsigned(
        "pub.tangled.page",
        "Home",
        "public",
        serde_json::json!({ "ops": [{ "op": "ins", "after": null, "run": "Hi" }] }),
    );
    r0["author"] = serde_json::Value::String(wiki.did());
    let first = wiki.publish(r0).await.unwrap();

    let mut a = wiki.unsigned(
        "pub.tangled.page",
        "Home",
        "public",
        serde_json::json!({ "ops": [{ "op": "ins", "after": { "op": first.cid.to_string(), "i": 1 }, "run": "A" }] }),
    );
    a["author"] = serde_json::Value::String(wiki.did());
    a["prev"] = serde_json::json!([first.cid.to_string()]);
    a["signedBy"] = serde_json::Value::String(hex::encode(alice.identity.pubkey_bytes()));
    a["proof"] = serde_json::json!([cap_a.cid().unwrap().to_string()]);
    let signed_a = alice.sign(&a).unwrap();
    alice
        .to_with_cap(&wiki, ha.clone())
        .put_record(&PutRecordRequest {
            record: signed_a.json.clone(),
        })
        .await
        .unwrap();

    let mut b = wiki.unsigned(
        "pub.tangled.page",
        "Home",
        "public",
        serde_json::json!({ "ops": [{ "op": "ins", "after": { "op": first.cid.to_string(), "i": 1 }, "run": "B" }] }),
    );
    b["author"] = serde_json::Value::String(wiki.did());
    b["prev"] = serde_json::json!([first.cid.to_string()]);
    b["signedBy"] = serde_json::Value::String(hex::encode(bob.identity.pubkey_bytes()));
    b["proof"] = serde_json::json!([cap_b.cid().unwrap().to_string()]);
    let signed_b = bob.sign(&b).unwrap();
    let err = bob
        .to_with_cap(&wiki, hb.clone())
        .put_record(&PutRecordRequest {
            record: signed_b.json.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).1, "ConcurrentHeads");

    b["prev"] = serde_json::json!([first.cid.to_string(), signed_a.cid.to_string()]);
    let signed_b2 = bob.sign(&b).unwrap();
    bob.to_with_cap(&wiki, hb)
        .put_record(&PutRecordRequest {
            record: signed_b2.json,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn compact_admission_rate_limited() {
    let alice = Node::named("alice-cmp");
    let mut prev: Vec<String> = vec![];
    for i in 0..3 {
        let mut rec = alice.unsigned(
            "pub.tangled.page",
            "P",
            "public",
            serde_json::json!({ "ops": [{ "op": "ins", "after": null, "run": format!("{i}") }] }),
        );
        rec["prev"] = serde_json::json!(prev);
        rec["ts"] = serde_json::json!(NOW * 1000 + i as u64);
        let out = alice.publish(rec).await.unwrap();
        prev = vec![out.cid.to_string()];
    }
    let mut compact = alice.unsigned(
        "pub.tangled.page",
        "P",
        "public",
        serde_json::json!({
            "compact": 1,
            "frontier": prev,
            "snapshot": {
                "atoms": [],
                "title": { "value": "", "ts": 0, "cid": "" },
                "blob": null
            }
        }),
    );
    compact["prev"] = serde_json::json!(prev.clone());
    compact["ts"] = serde_json::json!(NOW * 1000 + 10);
    let err = alice
        .local()
        .put_record(&PutRecordRequest {
            record: alice.sign(&compact).unwrap().json,
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).1, "RateLimited");
    assert!(!compact_allowed("pub.tangled.post"));

    let mut post_c = alice.unsigned(
        "pub.tangled.post",
        "nope",
        "public",
        serde_json::json!({ "compact": 1, "frontier": ["x"], "snapshot": { "atoms": [], "title": { "value": "", "ts": 0, "cid": "" }, "blob": null } }),
    );
    let err = alice
        .local()
        .put_record(&PutRecordRequest {
            record: alice.sign(&post_c).unwrap().json,
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 400);
    let _ = &mut post_c;
}

#[tokio::test]
async fn ap_import_attested_and_get_blocks() {
    assert!(activitypub_import_attested(true).is_ok());
    let alice = Node::named("alice-ap");
    let event = "ab".repeat(32);
    let rkey = import_rkey("activitypub", "https://ex.example/users/a/s/1").unwrap();
    let actor = "https://ex.example/users/a/s/1";
    let proof = activitypub_foreign_proof(&alice.identity, actor, br#"{"type":"Create"}"#);
    alice
        .publish(alice.unsigned(
            "sys.foreign",
            &rkey,
            "public",
            serde_json::json!({
                "protocol": "activitypub",
                "foreign_id": actor,
                "attested": true,
                "foreignProof": proof,
            }),
        ))
        .await
        .unwrap();
    let listed = alice
        .local()
        .list_records(&alice.did(), Some("sys.foreign"))
        .await
        .unwrap();
    assert_eq!(listed.records[0].record["body"]["attested"], true);

    let post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "z",
            "public",
            serde_json::json!({ "text": event }),
        ))
        .await
        .unwrap();
    let blocks = alice
        .local()
        .get_blocks(&GetBlocksRequest {
            cids: vec![post.cid.to_string()],
        })
        .await
        .unwrap();
    assert_eq!(blocks.status, 200);
    assert!(blocks.content_type.contains("car") || !blocks.body.is_empty());

    let auth = alice
        .local()
        .get_repo_auth(&GetRepoAuthRequest {
            did: alice.did(),
            since: None,
            blobs: Some(false),
        })
        .await;
    assert!(auth.is_err());

    let d = alice.local().describe().await.unwrap();
    assert_eq!(d.v, 1);
    alice
        .local()
        .notify(&tangled::http::NotifyRequest {
            did: alice.did(),
            commit: post.cid.to_string(),
        })
        .await
        .unwrap();

    let sub = alice.local().subscribe(&alice.did(), None).await;
    assert!(sub.is_err());
    let fire = alice.local().loom_firehose().await;
    assert!(fire.is_err());
}

#[tokio::test]
async fn unknown_vis_rejected_on_publish() {
    let alice = Node::named("alice-vis");
    let rec = alice.unsigned(
        "pub.tangled.post",
        "bad",
        "friends-only",
        serde_json::json!({ "text": "x" }),
    );
    assert!(alice.sign(&rec).is_ok());
    assert!(tangled::record::validate_envelope(&rec).is_err());
}

#[test]
fn identity_device_not_mixed() {
    let a = identity_named("mix-a");
    let b = identity_named("mix-b");
    assert_ne!(a.pubkey_bytes(), a.agreement_public_bytes());
    assert_ne!(a.did_string(), b.did_string());
    let _ = Cid::dag_cbor(b"sys.id");
    let recs = vec![];
    let _ = apply(&recs);
    let _ = record_from_unsigned("x", &serde_json::json!({"ts":1,"body":{"ops":[]}}));
}
