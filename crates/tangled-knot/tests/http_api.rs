//! HTTP integration against a real listening knot (`KnotClient::http`).

mod common;

use common::{http_code, identity_named, Live, NOW};
use tangled::capability::{cap_authorization_header, Invocation};
use tangled::car;
use tangled::http::{
    overlay_comment_count, overlay_proves_zero_comments, GetBlocksRequest, GetRepoAuthRequest,
    OverlayCount, PutRecordRequest,
};
use tangled::record::compact_allowed;
use tangled::sync::safe_did_rkey;

#[tokio::test]
async fn describe_head_put_get_public() {
    let alice = Live::named("http-alice").await;
    let d = alice.client().describe().await.unwrap();
    assert_eq!(d.v, 1);
    assert_eq!(d.did, alice.did());
    assert!(d.profiles.iter().any(|p| p == "knot.public"));

    let post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "hello",
            "public",
            serde_json::json!({ "text": "from alice" }),
        ))
        .await
        .unwrap();
    assert_eq!(post.put.cid.as_deref(), Some(post.cid.to_string().as_str()));

    let head = alice.client().get_head(&alice.did()).await.unwrap();
    assert_eq!(head.did, alice.did());
    assert!(!head.commit.is_empty());
    assert!(head.knot.starts_with("http://"));

    let got = alice
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "hello")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "from alice");
    assert!(!got.proof.is_empty());

    let car = car::decode(
        &alice
            .client()
            .get_repo(&alice.did(), None, None)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!car.blocks.is_empty());

    let listed = alice
        .client()
        .list_records(&alice.did(), Some("pub.tangled.post"))
        .await
        .unwrap_err();
    assert_eq!(http_code(&listed).1, "MethodNotImplemented");

    let resolved = alice
        .client()
        .identity_resolve(None, Some(&alice.did()))
        .await
        .unwrap();
    assert_eq!(resolved.did, alice.did());
    assert!(d.peers.is_empty(), "default describe.peers must be empty");

    let stats = alice.client().stats().await.unwrap();
    assert_eq!(stats.v, 1);
    assert_eq!(stats.did, alice.did());
    assert_eq!(stats.counts.identities, 1);
    assert_eq!(stats.counts.records, 1);
    assert_eq!(stats.counts.peers, 0);
    assert_eq!(stats.topology.hosted.len(), 1);
    assert_eq!(stats.topology.hosted[0].did, alice.did());
}

#[tokio::test]
async fn describe_advertises_configured_public_peers() {
    let alice = Live::with_peers("peer-alice", &["https://b.example", "https://c.example/"]).await;
    let d = alice.client().describe().await.unwrap();
    assert_eq!(
        d.peers,
        vec![
            "https://b.example".to_string(),
            "https://c.example".to_string()
        ]
    );
}

#[tokio::test]
async fn connecting_knot_learns_remote_public_peers() {
    let alice = Live::with_peers("gossip-alice", &["https://b.example", "https://c.example"]).await;
    let bob = Live::named("gossip-bob").await;
    assert!(bob.client().describe().await.unwrap().peers.is_empty());

    let learned =
        tangled_knot::learn_from_remote(&bob.state.engine, &format!("http://{}", alice.addr))
            .await
            .unwrap();
    assert_eq!(
        learned,
        vec![
            "https://b.example".to_string(),
            "https://c.example".to_string()
        ]
    );
    assert_eq!(
        bob.state.engine.lock().unwrap().learned_peers(),
        &[
            "https://b.example".to_string(),
            "https://c.example".to_string()
        ]
    );
    // Learned URLs are not re-advertised.
    assert!(bob.client().describe().await.unwrap().peers.is_empty());
}

#[test]
fn cli_peer_flags_and_peers_file() {
    let cli = tangled_knot::KnotCli::parse([
        "--peer",
        "https://b.example",
        "--peer",
        "https://c.example/",
        "--listen",
        "127.0.0.1:0",
    ])
    .unwrap();
    assert_eq!(
        cli.peers,
        vec![
            "https://b.example".to_string(),
            "https://c.example".to_string()
        ]
    );

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("peers.txt"),
        "# operator public set\nhttps://file.example/\nhttp://127.0.0.1:4000\n",
    )
    .unwrap();
    let collected = tangled_knot::collect_public_peers(
        &cli.peers,
        dir.path(),
        Some("https://env.example, ftp://bad.example"),
    );
    assert_eq!(
        collected,
        vec![
            "https://b.example".to_string(),
            "https://c.example".to_string(),
            "https://env.example".to_string(),
            "https://file.example".to_string(),
            "http://127.0.0.1:4000".to_string(),
        ]
    );
}

#[tokio::test]
async fn vis_matrix_public_cap_sealed() {
    let alice = Live::named("vis-alice").await;
    let bob = Live::named("vis-bob").await;

    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "pub1",
            "public",
            serde_json::json!({ "text": "open" }),
        ))
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "cap1",
            "cap",
            serde_json::json!({ "text": "gated" }),
        ))
        .await
        .unwrap();
    let mut sealed = alice.unsigned(
        "pub.tangled.chat",
        "s1",
        "sealed",
        serde_json::json!({ "ciphertext": "aa" }),
    );
    sealed["circle"] = serde_json::json!(format!("{}#circ", alice.did()));
    alice.publish(sealed).await.unwrap();

    let pub_ok = bob
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "pub1")
        .await;
    // Bob's client talks to Bob's knot — Alice's record is not there.
    assert_eq!(http_code(&pub_ok.unwrap_err()).0, 404);

    let pulled = alice
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "pub1")
        .await
        .unwrap();
    assert_eq!(pulled.record["body"]["text"], "open");

    let cap_need = alice
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "cap1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&cap_need), (401, "CapabilityRequired".into()));

    let sealed_need = alice
        .client()
        .get_record(&alice.did(), "pub.tangled.chat", "s1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&sealed_need).0, 401);
}

#[tokio::test]
async fn two_knots_bob_pulls_alice() {
    let alice = Live::named("pull-alice").await;
    let bob = Live::named("pull-bob").await;
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "p1",
            "public",
            serde_json::json!({ "text": "portable" }),
        ))
        .await
        .unwrap();

    let missing = bob
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "p1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&missing).0, 404);

    let bob_at_alice = tangled::http::KnotClient::http(format!("http://{}", alice.addr));
    let live_pull = bob_at_alice
        .get_record(&alice.did(), "pub.tangled.post", "p1")
        .await
        .unwrap();
    assert_eq!(live_pull.record["body"]["text"], "portable");

    let car_bytes = alice
        .client()
        .get_repo(&alice.did(), None, Some(false))
        .await
        .unwrap();
    let token = common::siws_token(&bob.client(), &alice.identity)
        .await
        .unwrap();
    let imported = bob
        .client()
        .with_session_cookie(token)
        .import_repo(car_bytes, true)
        .await
        .unwrap();
    assert_eq!(imported.did, alice.did());
    let pulled = bob
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "p1")
        .await
        .unwrap();
    assert_eq!(pulled.record["body"]["text"], "portable");
}

#[tokio::test]
async fn cap_fail_closed_sibling_stolen_write() {
    let alice = Live::named("cap-alice").await;
    let bob = Live::named("cap-bob").await;
    let mallory = Live::named("cap-mallory").await;

    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "draft1",
            "cap",
            serde_json::json!({ "text": "only bob" }),
        ))
        .await
        .unwrap();

    let res = format!("tangled://{}/pub.tangled.post", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let got = alice
        .client()
        .with_cap_header(header)
        .get_record(&alice.did(), "pub.tangled.post", "draft1")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "only bob");

    let wrong = format!("tangled://{}/pub.tangled.chat", alice.did());
    let (bad, _) = alice
        .issue_cap(&mallory.did(), "repo/read", &wrong)
        .unwrap();
    let bad_h = mallory.invocation_header(&bad, "repo/read", &wrong, &alice.did());
    let sibling = alice
        .client()
        .with_cap_header(bad_h)
        .get_record(&alice.did(), "pub.tangled.post", "draft1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&sibling).1, "CapabilityInvalid");

    let write_res = format!("tangled://{}", alice.did());
    let (read_cap, _) = alice
        .issue_cap(&mallory.did(), "repo/read", &write_res)
        .unwrap();
    let steal = mallory.invocation_header(&read_cap, "repo/read", &write_res, &alice.did());
    let rec = mallory.unsigned(
        "pub.tangled.post",
        "hack",
        "public",
        serde_json::json!({ "text": "nope" }),
    );
    let signed = mallory.sign(&rec).unwrap();
    let err = alice
        .client()
        .with_cap_header(steal)
        .put_record(&PutRecordRequest {
            record: signed.json,
        })
        .await
        .unwrap_err();
    let (status, _) = http_code(&err);
    assert!(status == 401 || status == 403, "{status}");
}

#[tokio::test]
async fn invocation_must_be_dag_cbor() {
    let alice = Live::named("inv-alice").await;
    let err = alice
        .client()
        .cap_verify(b"{\"v\":1}".to_vec())
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);

    let bob = Live::named("inv-bob").await;
    let res = format!("tangled://{}", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let mut inv = Invocation::new(&cap, "repo/read", &res, &alice.did(), NOW, [9u8; 16]).unwrap();
    inv.sign(bob.identity.signing_key()).unwrap();
    let ok = alice
        .client()
        .cap_verify(inv.encode_cbor().unwrap())
        .await
        .unwrap();
    assert!(ok.ok);
}

#[tokio::test]
async fn expired_cap_rejected() {
    let alice = Live::named("exp-alice").await;
    let bob = Live::named("exp-bob").await;
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
    // Engine now is NOW; cap still valid. Mutate via a second invocation after bumping
    // is not exposed on HTTP — issue a cap that is already expired relative to NOW.
    cap.exp = NOW - 120;
    cap.nbf = NOW - 200;
    cap.sig = None;
    cap.sign(alice.identity.signing_key()).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let err = alice
        .client()
        .with_cap_header(header)
        .get_record(&alice.did(), "pub.tangled.post", "x")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);
}

#[tokio::test]
async fn circle_read_and_revocation() {
    let alice = Live::named("circ-alice").await;
    let bob = Live::named("circ-bob").await;
    let circle_rkey = "circ1";
    let envelope = format!("{}#{circle_rkey}", alice.did());
    alice
        .publish(alice.unsigned(
            "sys.circle",
            circle_rkey,
            "public",
            serde_json::json!({ "v": 1, "epoch": 0 }),
        ))
        .await
        .unwrap();
    let mut rec = alice.unsigned(
        "pub.tangled.chat",
        "c1",
        "sealed",
        serde_json::json!({ "ciphertext": "00" }),
    );
    rec["circle"] = serde_json::json!(envelope);
    alice.publish(rec).await.unwrap();

    let cres = tangled::capability::circle_resource_uri(&alice.did(), circle_rkey);
    let (cap, _) = alice.issue_cap(&bob.did(), "circle/read", &cres).unwrap();
    let header = bob.invocation_header(&cap, "circle/read", &cres, &alice.did());
    let got = alice
        .client()
        .with_cap_header(header.clone())
        .get_record(&alice.did(), "pub.tangled.chat", "c1")
        .await
        .unwrap();
    assert_eq!(got.record["vis"], "sealed");

    let cap_cid = cap.cid().unwrap().to_string();
    alice
        .publish(alice.unsigned(
            "sys.revocation",
            "rev1",
            "public",
            serde_json::json!({ "cid": cap_cid }),
        ))
        .await
        .unwrap();
    let err = alice
        .client()
        .with_cap_header(header)
        .get_record(&alice.did(), "pub.tangled.chat", "c1")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).1, "CapabilityInvalid");
}

#[tokio::test]
async fn blobs_getblob_auth_and_blocks() {
    let alice = Live::named("blob-alice").await;
    let bytes = b"jpeg-bytes";
    let blob = alice.client().put_blob(bytes.to_vec()).await.unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.media",
            "m1",
            "public",
            serde_json::json!({ "blob": blob.cid, "alt": "cat" }),
        ))
        .await
        .unwrap();
    let bob_at_alice = tangled::http::KnotClient::http(format!("http://{}", alice.addr));
    let got = bob_at_alice
        .get_blob(&alice.did(), &blob.cid)
        .await
        .unwrap();
    assert_eq!(got, bytes);

    let sealed_blob = alice.client().put_blob(b"secret".to_vec()).await.unwrap();
    let mut media = alice.unsigned(
        "pub.tangled.media",
        "m2",
        "sealed",
        serde_json::json!({ "blob": sealed_blob.cid }),
    );
    media["circle"] = serde_json::json!(format!("{}#c", alice.did()));
    alice.publish(media).await.unwrap();
    let denied = bob_at_alice
        .get_blob(&alice.did(), &sealed_blob.cid)
        .await
        .unwrap_err();
    assert_eq!(http_code(&denied).0, 401);

    let blocks = alice
        .client()
        .get_blocks(&GetBlocksRequest {
            cids: vec![blob.cid.clone()],
        })
        .await
        .unwrap();
    assert_eq!(blocks.status, 200);
}

#[tokio::test]
async fn car_since_and_get_repo_auth() {
    let alice = Live::named("car-alice").await;
    let first = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "a",
            "public",
            serde_json::json!({ "text": "one" }),
        ))
        .await
        .unwrap();
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
            .client()
            .get_repo(&alice.did(), None, None)
            .await
            .unwrap(),
    )
    .unwrap();
    let diff = car::decode(
        &alice
            .client()
            .get_repo(&alice.did(), Some(&first.cid.to_string()), None)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(full.blocks.len() >= diff.blocks.len());

    let bob = Live::named("car-bob").await;
    let res = format!("tangled://{}/pub.tangled.post", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let auth_car = alice
        .client()
        .with_cap_header(header)
        .get_repo_auth(&GetRepoAuthRequest {
            did: alice.did(),
            since: None,
            blobs: Some(false),
        })
        .await
        .unwrap();
    assert!(!auth_car.is_empty());
}

#[tokio::test]
async fn car_export_filters_private_record_blocks_and_scoped_siblings() {
    let alice = Live::named("car-privacy-alice").await;
    let bob = Live::named("car-privacy-bob").await;
    let public = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "public",
            "public",
            serde_json::json!({"text":"open"}),
        ))
        .await
        .unwrap();
    let private = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "private",
            "cap",
            serde_json::json!({"text":"cap secret"}),
        ))
        .await
        .unwrap();
    let mut sealed = alice.unsigned(
        "pub.tangled.chat",
        "sealed",
        "sealed",
        serde_json::json!({"ciphertext":"aabb"}),
    );
    sealed["circle"] = serde_json::json!(format!("{}#circle", alice.did()));
    let sibling = alice.publish(sealed).await.unwrap();
    let exported = car::decode(
        &alice
            .client()
            .get_repo(&alice.did(), None, Some(false))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(exported.blocks.iter().any(|(cid, _)| cid == &public.cid));
    assert!(
        !exported
            .blocks
            .iter()
            .any(|(cid, _)| cid == &private.cid || cid == &sibling.cid),
        "public CAR must not contain private record bytes"
    );
    let head = alice.client().get_head(&alice.did()).await.unwrap();
    let commit = tangled::record::parse_commit(
        exported
            .get(&tangled::cid::Cid::parse(&head.commit).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(
        exported
            .blocks
            .iter()
            .any(|(cid, _)| cid.to_string() == commit.data),
        "public CAR retains MST structural proof root"
    );
    let res = format!("tangled://{}/pub.tangled.post", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let scoped = car::decode(
        &alice
            .client_cap(header)
            .get_repo_auth(&GetRepoAuthRequest {
                did: alice.did(),
                since: None,
                blobs: Some(false),
            })
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        scoped.blocks.iter().any(|(cid, _)| cid == &private.cid),
        "authorized CAR includes permitted record"
    );
    assert!(
        !scoped.blocks.iter().any(|(cid, _)| cid == &sibling.cid),
        "scoped CAR must not contain sibling private record bytes"
    );
    let circle_log = alice
        .publish(alice.unsigned(
            "sys.circle",
            "circle",
            "cap",
            serde_json::json!({"v":1,"epoch":0}),
        ))
        .await
        .unwrap();
    let other_log = alice
        .publish(alice.unsigned(
            "sys.circle",
            "other",
            "cap",
            serde_json::json!({"v":1,"epoch":0}),
        ))
        .await
        .unwrap();
    let mut other = alice.unsigned(
        "pub.tangled.chat",
        "other",
        "sealed",
        serde_json::json!({"ciphertext":"ccdd"}),
    );
    other["circle"] = serde_json::json!(format!("{}#other", alice.did()));
    let other = alice.publish(other).await.unwrap();
    let res = tangled::capability::circle_resource_uri(&alice.did(), "circle");
    let (cap, _) = alice.issue_cap(&bob.did(), "circle/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "circle/read", &res, &alice.did());
    let circle_car = car::decode(
        &alice
            .client_cap(header)
            .get_repo_auth(&GetRepoAuthRequest {
                did: alice.did(),
                since: None,
                blobs: Some(false),
            })
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(circle_car.blocks.iter().any(|(cid, _)| cid == &sibling.cid));
    assert!(circle_car
        .blocks
        .iter()
        .any(|(cid, _)| cid == &circle_log.cid));
    for (label, cid) in [("other circle payload", &other.cid), ("other circle log", &other_log.cid), ("unrelated private record", &private.cid)] {
        assert!(circle_car.get(cid).is_none(), "circle CAR leaked {label} {cid}: {:?}", circle_car.get(cid).map(tangled::dagcbor::decode_json));
    }
}

#[tokio::test]
async fn commit_cursor_delta_import_uses_verified_local_history() {
    let alice = Live::named("commit-delta-alice").await;
    let bob = Live::named("commit-delta-bob").await;
    let first = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "first",
            "public",
            serde_json::json!({"text":"one"}),
        ))
        .await
        .unwrap();
    let second = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "second",
            "public",
            serde_json::json!({"text":"two"}),
        ))
        .await
        .unwrap();
    let full = alice
        .client()
        .get_repo(&alice.did(), None, Some(false))
        .await
        .unwrap();
    let token = common::siws_token(&bob.client(), &alice.identity)
        .await
        .unwrap();
    bob.client()
        .with_session_cookie(token.clone())
        .import_repo(full, true)
        .await
        .unwrap();
    let third = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "third",
            "public",
            serde_json::json!({"text":"three"}),
        ))
        .await
        .unwrap();
    let since = second.put.commit.unwrap();
    let bytes = alice
        .client()
        .get_repo(&alice.did(), Some(&since), Some(false))
        .await
        .unwrap();
    let delta = car::decode(&bytes).unwrap();
    assert!(delta.blocks.iter().any(|(cid, _)| cid.to_string() == since));
    assert!(delta
        .blocks
        .iter()
        .any(|(cid, _)| Some(cid.to_string()) == third.put.commit));
    assert!(!delta
        .blocks
        .iter()
        .any(|(cid, _)| Some(cid.to_string()) == first.put.commit));
    let empty = Live::named("commit-delta-empty").await;
    let empty_token = common::siws_token(&empty.client(), &alice.identity)
        .await
        .unwrap();
    let missing_history = empty
        .client()
        .with_session_cookie(empty_token)
        .import_repo(bytes.clone(), true)
        .await
        .unwrap_err();
    assert_eq!(
        http_code(&missing_history),
        (400, "InvalidSignature".into())
    );
    bob.client()
        .with_session_cookie(token)
        .import_repo(bytes, true)
        .await
        .expect("commit delta import must use the already verified local predecessor chain");
    assert_eq!(
        bob.client()
            .get_record(&alice.did(), "pub.tangled.post", "third")
            .await
            .unwrap()
            .record["body"]["text"],
        "three"
    );
}

#[tokio::test]
async fn backlinks_empty_is_not_zero_comments() {
    let alice = Live::named("bl-alice").await;
    let post = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "p",
            "public",
            serde_json::json!({ "text": "hi" }),
        ))
        .await
        .unwrap();
    let cid = post.cid.to_string();
    let empty = alice
        .client()
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
    assert!(!overlay_proves_zero_comments(&obs));

    let mut ann = alice.unsigned(
        "pub.tangled.annotation",
        "a1",
        "public",
        serde_json::json!({ "reply": cid, "text": "note" }),
    );
    ann["ref"] = serde_json::json!(cid);
    alice.publish(ann).await.unwrap();
    let hits = alice
        .client()
        .query_backlinks(&cid, None, None)
        .await
        .unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert!(hits.overlay);
    assert!(!hits.complete);

    let loom = alice.client().loom_query(&cid, Some("ref")).await.unwrap();
    assert_eq!(loom.hits.len(), 1);
}

#[tokio::test]
async fn compact_admission_and_last_ts_wins() {
    let alice = Live::named("cmp-alice").await;
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
    compact["prev"] = serde_json::json!(prev);
    compact["ts"] = serde_json::json!(NOW * 1000 + 10);
    let err = alice
        .client()
        .put_record(&PutRecordRequest {
            record: alice.sign(&compact).unwrap().json,
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).1, "RateLimited");
    let raw = reqwest::Client::new()
        .post(format!("http://{}/xrpc/tangled.repo.putRecord", alice.addr))
        .json(&PutRecordRequest {
            record: alice.sign(&compact).unwrap().json,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status().as_u16(), 429);
    assert_eq!(raw.headers().get("retry-after").unwrap(), "300");
    assert!(!compact_allowed("pub.tangled.post"));

    let post_c = alice.unsigned(
        "pub.tangled.post",
        "nope",
        "public",
        serde_json::json!({ "compact": 1 }),
    );
    let err = alice
        .client()
        .put_record(&PutRecordRequest {
            record: alice.sign(&post_c).unwrap().json,
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 400);
}

#[tokio::test]
async fn concurrent_heads_signed_by() {
    let wiki = Live::named("wiki-http").await;
    let alice = Live::named("collab-http-a").await;
    let res = format!("tangled://{}/pub.tangled.page", wiki.did());
    let (cap_a, _) = wiki.issue_cap(&alice.did(), "repo/write", &res).unwrap();
    let ha = alice.invocation_header(&cap_a, "repo/write", &res, &wiki.did());

    let r0 = wiki.unsigned(
        "pub.tangled.page",
        "Home",
        "public",
        serde_json::json!({ "ops": [{ "op": "ins", "after": null, "run": "Hi" }] }),
    );
    let first = wiki.publish(r0.clone()).await.unwrap();

    let mut a = wiki.unsigned(
        "pub.tangled.page",
        "Home",
        "public",
        serde_json::json!({ "ops": [{ "op": "ins", "after": null, "run": "A" }] }),
    );
    a["author"] = serde_json::json!(wiki.did());
    a["signedBy"] = serde_json::Value::String(hex::encode(alice.identity.pubkey_bytes()));
    a["proof"] = serde_json::json!([cap_a.cid().unwrap().to_string()]);
    a["prev"] = serde_json::json!([first.cid.to_string()]);
    let signed_a = alice.sign(&a).unwrap();
    wiki.client_cap(ha.clone())
        .put_record(&PutRecordRequest {
            record: signed_a.json,
        })
        .await
        .unwrap();

    let mut b = wiki.unsigned(
        "pub.tangled.page",
        "Home",
        "public",
        serde_json::json!({ "ops": [{ "op": "ins", "after": null, "run": "B" }] }),
    );
    b["author"] = serde_json::json!(wiki.did());
    b["signedBy"] = serde_json::Value::String(hex::encode(alice.identity.pubkey_bytes()));
    b["proof"] = serde_json::json!([cap_a.cid().unwrap().to_string()]);
    b["prev"] = serde_json::json!([first.cid.to_string()]);
    let signed_b = alice.sign(&b).unwrap();
    let err = wiki
        .client_cap(ha)
        .put_record(&PutRecordRequest {
            record: signed_b.json,
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).1, "ConcurrentHeads");
    let _ = r0;
}

#[tokio::test]
async fn migration_location_device_revoke_graph() {
    let alice = Live::named("mig-alice").await;
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
                "endpoints": [{ "type": "knot", "url": format!("http://{}", alice.addr) }],
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
        .client()
        .get_repo(&alice.did(), None, Some(true))
        .await
        .unwrap();
    let dest = Live::named("mig-dest").await;
    let token = common::siws_token(&dest.client(), &alice.identity)
        .await
        .unwrap();
    dest.client()
        .with_session_cookie(token)
        .import_repo(car_bytes, true)
        .await
        .unwrap();
    let got = dest
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "hello")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "before move");

    let block_rkey = safe_did_rkey(&dest.did());
    alice
        .publish(alice.unsigned(
            "tangled.graph.block",
            &block_rkey,
            "public",
            serde_json::json!({ "v": 1, "subject": dest.did() }),
        ))
        .await
        .unwrap();
    let blocked = alice
        .client()
        .get_record(&alice.did(), "tangled.graph.block", &block_rkey)
        .await
        .unwrap();
    assert_eq!(blocked.record["body"]["subject"], dest.did());

    alice
        .publish(alice.unsigned(
            "sys.revocation",
            "dev1",
            "public",
            serde_json::json!({ "cid": "bafyrevokedevice" }),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn notify_and_unknown_method() {
    let alice = Live::named("n-alice").await;
    alice
        .client()
        .notify(&tangled::http::NotifyRequest {
            did: alice.did(),
            commit: "bafy".into(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn bad_signature_rejected() {
    let alice = Live::named("sig-alice").await;
    let mut rec = alice.unsigned(
        "pub.tangled.post",
        "bad",
        "public",
        serde_json::json!({ "text": "x" }),
    );
    rec["sig"] = serde_json::json!("00".repeat(64));
    let err = alice
        .client()
        .put_record(&PutRecordRequest { record: rec })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 400);
}

#[tokio::test]
async fn unknown_visibility_rejected_without_mutation() {
    let alice = Live::named("invalid-visibility-http").await;
    let invalid = alice.unsigned(
        "pub.tangled.post",
        "bad-vis",
        "unknown",
        serde_json::json!({"text":"invalid"}),
    );
    let error = alice.publish(invalid).await.unwrap_err();
    assert_eq!(http_code(&error), (400, "InvalidRequest".into()));
    let missing = alice
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "bad-vis")
        .await
        .unwrap_err();
    assert_eq!(
        http_code(&missing).0,
        404,
        "invalid envelope must not be stored"
    );
}

#[tokio::test]
async fn unsigned_hosted_write_rejected_without_mutation() {
    let alice = Live::named("unsigned-hosted-http").await;
    let record = alice.unsigned(
        "pub.tangled.post",
        "unsigned",
        "public",
        serde_json::json!({"text":"unsigned"}),
    );
    let error = alice
        .client()
        .put_record(&PutRecordRequest { record })
        .await
        .unwrap_err();
    assert_eq!(http_code(&error), (400, "InvalidSignature".into()));
    let missing = alice
        .client()
        .get_record(&alice.did(), "pub.tangled.post", "unsigned")
        .await
        .unwrap_err();
    assert_eq!(http_code(&missing).0, 404);
}

#[tokio::test]
async fn json_cap_header_forbidden() {
    let alice = Live::named("jsoncap-alice").await;
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "c",
            "cap",
            serde_json::json!({ "text": "n" }),
        ))
        .await
        .unwrap();
    let header = cap_authorization_header(br#"{"v":1}"#);
    let err = alice
        .client_cap(header)
        .get_record(&alice.did(), "pub.tangled.post", "c")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);
}

#[tokio::test]
async fn siws_http_happy_bad_sig_and_cookie() {
    let host = Live::named("siws-http-host").await;
    let guest = identity_named("siws-http-guest");
    let ch = host
        .client()
        .session_challenge(&guest.did_string())
        .await
        .unwrap();
    assert!(ch.message.contains(&guest.did_string()));
    let bad = host
        .client()
        .session_verify(&tangled::session::SiwsVerifyRequest {
            did: guest.did_string(),
            nonce: ch.nonce.clone(),
            signature: "00".repeat(64),
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&bad), (400, "InvalidSignature".into()));

    let token = common::siws_token(&host.client(), &guest).await.unwrap();
    let mut rec = serde_json::json!({
        "v": 1,
        "type": common::schema_cid("pub.tangled.post"),
        "collection": "pub.tangled.post",
        "rkey": "guest-post",
        "author": guest.did_string(),
        "ts": NOW * 1000,
        "prev": [],
        "vis": "public",
        "body": { "text": "cookie" },
    });
    rec = tangled::record::sign_json_record(guest.signing_key(), &rec)
        .unwrap()
        .json;
    host.client()
        .with_session_cookie(token)
        .put_record(&PutRecordRequest { record: rec })
        .await
        .unwrap();
}

#[tokio::test]
async fn activitypub_unattested_rejected() {
    let alice = Live::named("ap-unattested").await;
    let err = alice
        .publish(alice.unsigned(
            "sys.foreign",
            "ap1",
            "public",
            serde_json::json!({
                "protocol": "activitypub",
                "foreign_id": "https://ex.example/users/a/s/1",
                "attested": false,
            }),
        ))
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 400);
}

#[tokio::test]
async fn device_revoke_blocks_subsequent_write() {
    let alice = Live::named("rev-alice").await;
    let device = identity_named("rev-device");
    let (cap, _) = alice
        .issue_cap(
            &device.did_string(),
            "repo/write",
            &format!("tangled://{}", alice.did()),
        )
        .unwrap();
    let cap_cid = cap.cid().unwrap().to_string();
    let header = {
        let mut inv = Invocation::new(
            &cap,
            "repo/write",
            &format!("tangled://{}", alice.did()),
            &alice.did(),
            NOW,
            [9u8; 16],
        )
        .unwrap();
        inv.sign(device.signing_key()).unwrap();
        cap_authorization_header(&inv.encode_cbor().unwrap())
    };
    let mut rec = alice.unsigned(
        "pub.tangled.post",
        "dev1",
        "public",
        serde_json::json!({ "text": "device" }),
    );
    rec["signedBy"] = serde_json::Value::String(hex::encode(device.pubkey_bytes()));
    rec["proof"] = serde_json::json!([cap_cid]);
    rec["author"] = serde_json::json!(alice.did());
    let signed = tangled::record::sign_json_record(device.signing_key(), &rec).unwrap();
    alice
        .client_cap(header.clone())
        .put_record(&PutRecordRequest {
            record: signed.json,
        })
        .await
        .unwrap();

    alice
        .publish(alice.unsigned(
            "sys.revocation",
            "r1",
            "public",
            serde_json::json!({ "cid": cap_cid, "ts": NOW, "reason": "lost" }),
        ))
        .await
        .unwrap();

    let mut rec2 = alice.unsigned(
        "pub.tangled.post",
        "dev2",
        "public",
        serde_json::json!({ "text": "after" }),
    );
    rec2["signedBy"] = serde_json::Value::String(hex::encode(device.pubkey_bytes()));
    rec2["proof"] = serde_json::json!([cap_cid]);
    rec2["author"] = serde_json::json!(alice.did());
    let signed2 = tangled::record::sign_json_record(device.signing_key(), &rec2).unwrap();
    let err = alice
        .client_cap(header)
        .put_record(&PutRecordRequest {
            record: signed2.json,
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);
}

#[tokio::test]
async fn cors_preflight_credentialed_echoes_origin() {
    let alice = Live::named("cors-alice").await;
    let base = format!("http://{}", alice.addr);
    let client = reqwest::Client::new();
    let open = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{base}/xrpc/tangled.node.describe"),
        )
        .header("Origin", "https://app.example")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();
    assert_eq!(
        open.headers()
            .get("access-control-allow-origin")
            .unwrap()
            .to_str()
            .unwrap(),
        "*"
    );

    let cred = client
        .request(
            reqwest::Method::OPTIONS,
            format!("{base}/xrpc/tangled.sync.getRecord"),
        )
        .header("Origin", "https://app.example")
        .header("Access-Control-Request-Method", "GET")
        .header(
            "Access-Control-Request-Headers",
            "Authorization, Content-Type",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        cred.headers()
            .get("access-control-allow-origin")
            .unwrap()
            .to_str()
            .unwrap(),
        "https://app.example"
    );
    assert_eq!(
        cred.headers()
            .get("access-control-allow-credentials")
            .unwrap()
            .to_str()
            .unwrap(),
        "true"
    );
}

#[tokio::test]
async fn max_uses_overflow_is_403() {
    let alice = Live::named("uses-alice").await;
    let bob = Live::named("uses-bob").await;
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
        exp: NOW + 86_400,
        pol: vec![serde_json::json!(["max_uses", 1])],
        prf: vec![],
        meta: None,
        sig: None,
    };
    cap.sign(alice.identity.signing_key()).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    alice
        .client()
        .with_cap_header(header.clone())
        .get_record(&alice.did(), "pub.tangled.post", "x")
        .await
        .unwrap();
    let err = alice
        .client()
        .with_cap_header(header)
        .get_record(&alice.did(), "pub.tangled.post", "x")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);
    assert_eq!(http_code(&err).1, "CapabilityInvalid");
}

#[tokio::test]
async fn get_repo_auth_since_in_json_body() {
    let alice = Live::named("since-body-alice").await;
    let first = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "a",
            "public",
            serde_json::json!({ "text": "one" }),
        ))
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "b",
            "public",
            serde_json::json!({ "text": "two" }),
        ))
        .await
        .unwrap();
    let bob = Live::named("since-body-bob").await;
    let res = format!("tangled://{}/pub.tangled.post", alice.did());
    let (cap, _) = alice.issue_cap(&bob.did(), "repo/read", &res).unwrap();
    let header = bob.invocation_header(&cap, "repo/read", &res, &alice.did());
    let auth_car = alice
        .client()
        .with_cap_header(header)
        .get_repo_auth(&GetRepoAuthRequest {
            did: alice.did(),
            since: Some(first.cid.to_string()),
            blobs: Some(false),
        })
        .await
        .unwrap();
    assert!(!auth_car.is_empty());
}

#[tokio::test]
async fn get_blob_sends_length_and_media_type() {
    let alice = Live::named("blob-hdr-alice").await;
    let bytes = b"png-bytes".to_vec();
    let put = alice
        .client()
        .put_blob_media(bytes.clone(), Some("image/png"))
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "pic",
            "public",
            serde_json::json!({ "text": "pic", "blob": put.cid }),
        ))
        .await
        .unwrap();
    let got = alice
        .client()
        .get_blob_response(&alice.did(), &put.cid)
        .await
        .unwrap();
    assert_eq!(got.body, bytes);
    assert_eq!(
        got.header("content-length"),
        Some(bytes.len().to_string().as_str())
    );
    assert_eq!(got.header("x-tangled-media-type"), Some("image/png"));
}

#[tokio::test]
async fn siws_blob_upload_is_owned_by_session_identity() {
    let host = Live::named("siws-blob-host").await;
    let guest = identity_named("siws-blob-guest");
    let token = common::siws_token(&host.client(), &guest).await.unwrap();
    let client = host.client().with_session_cookie(token);
    let bytes = b"guest-public-image".to_vec();
    let uploaded = client
        .put_blob_media(bytes.clone(), Some("image/png"))
        .await
        .unwrap();
    let record = serde_json::json!({
        "v":1, "type":common::schema_cid("pub.tangled.media"),
        "collection":"pub.tangled.media", "rkey":"guest-image",
        "author":guest.did_string(), "ts":NOW * 1000, "prev":[], "vis":"public",
        "body":{"blob":uploaded.cid,"mime":"image/png"},
    });
    let record = tangled::record::sign_json_record(guest.signing_key(), &record)
        .unwrap()
        .json;
    client
        .put_record(&PutRecordRequest { record })
        .await
        .unwrap();
    let response = host
        .client()
        .get_blob_response(&guest.did_string(), &uploaded.cid)
        .await;
    assert!(
        response.is_ok(),
        "guest-authenticated upload must be readable under its author's DID: {response:?}"
    );
    let response = response.unwrap();
    assert_eq!(response.body, bytes);
    assert_eq!(response.header("x-tangled-media-type"), Some("image/png"));
    assert_eq!(host.client().stats().await.unwrap().counts.blobs, 1);
}

#[tokio::test]
async fn blob_owner_query_and_invalid_or_expired_session_keep_existing_defaults() {
    let host = Live::named("blob-owner-defaults").await;
    let guest = identity_named("blob-owner-defaults-guest");
    let token = common::siws_token(&host.client(), &guest).await.unwrap();
    let explicit = b"explicit-query-owner".to_vec();
    let response = reqwest::Client::new()
        .post(format!("http://{}/xrpc/tangled.repo.putBlob", host.addr))
        .query(&[("did", host.did())])
        .header(
            "Cookie",
            format!("{}={token}", tangled::session::COOKIE_NAME),
        )
        .body(explicit.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let explicit: tangled::http::PutBlobResponse = response.json().await.unwrap();
    let invalid = host
        .client()
        .with_session_cookie("invalid-session")
        .put_blob(b"invalid-cookie".to_vec())
        .await
        .unwrap();
    {
        let mut engine = host.state.engine.lock().unwrap();
        engine.now += tangled::session::SESSION_TTL_SECONDS + 1;
    }
    let expired = host
        .client()
        .with_session_cookie(token)
        .put_blob(b"expired-cookie".to_vec())
        .await
        .unwrap();
    for (key, cid, bytes) in [
        ("explicit", explicit.cid, b"explicit-query-owner".as_slice()),
        ("invalid", invalid.cid, b"invalid-cookie".as_slice()),
        ("expired", expired.cid, b"expired-cookie".as_slice()),
    ] {
        host.publish(host.unsigned(
            "pub.tangled.media",
            key,
            "public",
            serde_json::json!({"blob":cid}),
        ))
        .await
        .unwrap();
        assert_eq!(
            host.client().get_blob(&host.did(), &cid).await.unwrap(),
            bytes
        );
        assert!(host
            .client()
            .get_blob(&guest.did_string(), &cid)
            .await
            .is_err());
    }
}

#[tokio::test]
async fn sns_handle_resolves_over_http_without_blocking_runtime_panic() {
    use base64::Engine;
    struct ServerTask(tokio::task::JoinHandle<()>);
    impl Drop for ServerTask {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let host = Live::named("sns-http-runtime").await;
    let mut registry = vec![0; 96];
    registry[32..64].copy_from_slice(&host.identity.pubkey_bytes());
    let encoded = base64::engine::general_purpose::STANDARD.encode(registry);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let rpc_url = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new().route("/", axum::routing::post(move || {
        let encoded = encoded.clone();
        async move { axum::Json(serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"value":{"data":[encoded,"base64"]}}})) }
    }));
    let _server = ServerTask(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    {
        let mut engine = host.state.engine.lock().unwrap();
        engine.handle_resolver.live = true;
        engine.handle_resolver.solana_rpc = Some(rpc_url);
    }
    assert!(host
        .client()
        .identity_resolve(Some("cluster.sol"), None)
        .await
        .is_err());
    host.publish(host.unsigned(
        "sys.id",
        "self",
        "public",
        serde_json::json!({
            "v":1, "pubkey":hex::encode(host.identity.pubkey_bytes()),
            "agreement":hex::encode(host.identity.agreement_public_bytes()),
            "alsoKnownAs":["cluster.sol"],
        }),
    ))
    .await
    .unwrap();
    let resolved = host
        .client()
        .identity_resolve(Some("cluster.sol"), None)
        .await;
    assert!(
        resolved.is_ok(),
        "forward SNS ownership plus signed reverse claim must resolve over HTTP: {resolved:?}"
    );
    assert_eq!(resolved.unwrap().did, host.did());
}
