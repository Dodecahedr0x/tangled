//! SIWS challenge/verify, cookie session, and putRecord authorization.

mod common;

use common::{http_code, identity_named, Node, NOW};
use tangled::http::PutRecordRequest;
use tangled::session::{self, SiwsVerifyRequest};
use tangled::KnotEngine;

#[tokio::test]
async fn siws_happy_path_cookie_put() {
    let host = Node::named("siws-host");
    let guest = identity_named("siws-guest");
    let token = common::siws_token(&host.local(), &guest).await.unwrap();
    let mut rec = serde_json::json!({
        "v": 1,
        "type": common::schema_cid("pub.tangled.post"),
        "collection": "pub.tangled.post",
        "rkey": "from-guest",
        "author": guest.did_string(),
        "ts": NOW * 1000,
        "prev": [],
        "vis": "public",
        "body": { "text": "via siws" },
    });
    let signed = tangled::record::sign_json_record(guest.signing_key(), &rec).unwrap();
    rec = signed.json;
    host.local()
        .with_session_cookie(token)
        .put_record(&PutRecordRequest { record: rec })
        .await
        .unwrap();
    let got = host
        .local()
        .get_record(&guest.did_string(), "pub.tangled.post", "from-guest")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "via siws");
}

#[tokio::test]
async fn siws_cookie_required_without_hosted_identity_key() {
    let host = Node::named("siws-req");
    let guest = identity_named("siws-req-guest");
    let mut rec = serde_json::json!({
        "v": 1,
        "type": common::schema_cid("pub.tangled.post"),
        "collection": "pub.tangled.post",
        "rkey": "no-cookie",
        "author": guest.did_string(),
        "ts": NOW * 1000,
        "prev": [],
        "vis": "public",
        "body": { "text": "nope" },
    });
    let signed = tangled::record::sign_json_record(guest.signing_key(), &rec).unwrap();
    rec = signed.json;
    let err = host
        .local()
        .put_record(&PutRecordRequest { record: rec })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 401);
}

#[tokio::test]
async fn siws_bad_signature() {
    let host = Node::named("siws-bad");
    let guest = identity_named("siws-bad-guest");
    let ch = host
        .local()
        .session_challenge(&guest.did_string())
        .await
        .unwrap();
    let err = host
        .local()
        .session_verify(&SiwsVerifyRequest {
            did: guest.did_string(),
            nonce: ch.nonce,
            signature: "00".repeat(64),
        })
        .await
        .unwrap_err();
    assert_eq!(http_code(&err), (400, "InvalidSignature".into()));
}

#[test]
fn siws_expired_challenge() {
    let mut engine = KnotEngine::hosting("did:sol:host");
    engine.now = NOW;
    let guest = identity_named("siws-exp");
    let req = tangled::http::KnotRequest {
        method: tangled::http::Method::Get,
        path: "/xrpc/tangled.session.challenge".into(),
        query: vec![("did".into(), guest.did_string())],
        headers: vec![],
        body: None,
        content_type: None,
    };
    engine.handle(&req).unwrap();
    engine.now = NOW + session::CHALLENGE_TTL_SECONDS + 1;
    let nonce = engine.challenges.keys().next().unwrap().clone();
    let ch = engine.challenges.get(&nonce).unwrap();
    let msg = session::siws_message(&ch.domain, &ch.did, &nonce, ch.issued_at);
    let sig = session::sign_siws(guest.signing_key(), &msg);
    let body = serde_json::to_vec(&SiwsVerifyRequest {
        did: guest.did_string(),
        nonce,
        signature: sig,
    })
    .unwrap();
    let resp = engine
        .handle(&tangled::http::KnotRequest {
            method: tangled::http::Method::Post,
            path: "/xrpc/tangled.session.verify".into(),
            query: vec![],
            headers: vec![],
            body: Some(body),
            content_type: Some("application/json".into()),
        })
        .unwrap();
    assert_eq!(resp.status, 400);
    let err: tangled::http::ApiError = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(err.error, "InvalidRequest");
    assert!(err.message.contains("expired"));
}

#[tokio::test]
async fn handle_one_sided_resolve_fails() {
    let alice = Node::named("resolve-alice");
    let handle = "alice.example.com";
    {
        let mut g = alice.store.lock().unwrap();
        g.handle_resolver
            .txt
            .insert(tangled::handle::txt_name(handle), vec![alice.did()]);
    }
    let err = alice
        .local()
        .identity_resolve(Some(handle), None)
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 404);
}

#[tokio::test]
async fn https_session_cookie_is_secure() {
    let guest = identity_named("siws-https");
    let mut engine = KnotEngine::hosting(&guest.did_string());
    engine.now = NOW;
    engine = engine.with_knot_base("https://knot.example");
    let ch = engine
        .handle(&tangled::http::KnotRequest {
            method: tangled::http::Method::Get,
            path: "/xrpc/tangled.session.challenge".into(),
            query: vec![("did".into(), guest.did_string())],
            headers: vec![],
            body: None,
            content_type: None,
        })
        .unwrap();
    let challenge: tangled::session::SiwsChallenge = serde_json::from_slice(&ch.body).unwrap();
    let sig = session::sign_siws(guest.signing_key(), &challenge.message);
    let body = serde_json::to_vec(&SiwsVerifyRequest {
        did: guest.did_string(),
        nonce: challenge.nonce,
        signature: sig,
    })
    .unwrap();
    let resp = engine
        .handle(&tangled::http::KnotRequest {
            method: tangled::http::Method::Post,
            path: "/xrpc/tangled.session.verify".into(),
            query: vec![],
            headers: vec![],
            body: Some(body),
            content_type: Some("application/json".into()),
        })
        .unwrap();
    let cookie = resp.set_cookie.expect("set-cookie");
    assert!(cookie.contains("Secure"), "{cookie}");
    assert!(cookie.contains("HttpOnly"));
}
