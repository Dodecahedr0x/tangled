//! ActivityPub S2S inbox → attested `sys.foreign` + mapped records.

mod common;

use common::{identity_named, Live};
use rand::rngs::OsRng;
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use tangled::car;
use tangled::foreign::{activitypub_actor_identity, import_rkey};
use tangled_ap::httpsig::{digest_header, http_date, sign_header_list, sign_request, unix_now};
use tangled_ap::{announce_activity, follow_activity, like_activity};

fn rsa() -> RsaPrivateKey {
    RsaPrivateKey::new(&mut OsRng, 2048).unwrap()
}

fn pem(key: &RsaPrivateKey) -> String {
    RsaPublicKey::from(key)
        .to_public_key_pem(LineEnding::LF)
        .unwrap()
}

fn actor_obj(id: &str, key: &RsaPrivateKey, inbox: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "Person",
        "inbox": inbox,
        "preferredUsername": "alice",
        "name": "Alice",
        "summary": "<p>Writes notes</p>",
        "publicKey": {
            "id": format!("{id}#main-key"),
            "owner": id,
            "publicKeyPem": pem(key)
        }
    })
}

async fn post_signed(
    url: &str,
    path: &str,
    host: &str,
    key: &RsaPrivateKey,
    key_id: &str,
    body: &[u8],
) -> reqwest::Response {
    let date = http_date(unix_now());
    let (signature, digest) = sign_request(key, key_id, "POST", path, host, &date, body);
    reqwest::Client::new()
        .post(url)
        .header("Host", host)
        .header("Date", date)
        .header("Digest", digest)
        .header("Signature", signature)
        .header("Content-Type", "application/activity+json")
        .body(body.to_vec())
        .send()
        .await
        .unwrap()
}

fn seed(knot: &Live, actor_id: &str, key: &RsaPrivateKey, inbox: &str) {
    knot.seed_actor(actor_obj(actor_id, key, inbox));
}

fn collections_in_repo(bytes: &[u8]) -> Vec<String> {
    let car = car::decode(bytes).unwrap();
    let mut out = Vec::new();
    for (_, data) in &car.blocks {
        if let Ok(v) = tangled::dagcbor::decode_json(data) {
            if let Some(c) = v.get("collection").and_then(|x| x.as_str()) {
                out.push(c.to_string());
            }
        }
    }
    out
}

#[tokio::test]
async fn describe_advertises_activitypub() {
    let knot = Live::named("ap-describe").await;
    let d = knot.client().describe().await.unwrap();
    assert!(
        d.protocols.iter().any(|p| p == "activitypub"),
        "protocols: {:?}",
        d.protocols
    );
}

#[tokio::test]
async fn webfinger_and_actor() {
    let knot = Live::named("ap-wf").await;
    let host = format!("{}", knot.addr);
    let url = format!("http://{host}/.well-known/webfinger?resource=acct:gateway@{host}");
    let jrd: serde_json::Value = reqwest::get(&url).await.unwrap().json().await.unwrap();
    let parsed = tangled_ap::parse_jrd(&jrd).unwrap();
    assert!(parsed.actor.contains("/users/gateway"));
    let actor: serde_json::Value = reqwest::get(&parsed.actor)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(actor["type"], "Person");
    assert!(actor["inbox"].as_str().unwrap().ends_with("/inbox"));
}

#[tokio::test]
async fn inbox_create_note_writes_attested_foreign_and_post() {
    let knot = Live::named("ap-note").await;
    let key = rsa();
    let actor_id = "https://ex.example/users/alice";
    seed(
        &knot,
        actor_id,
        &key,
        "https://ex.example/users/alice/inbox",
    );
    let note_id = "https://ex.example/users/alice/statuses/1";
    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": "https://ex.example/users/alice/statuses/1/activity",
        "type": "Create",
        "actor": actor_obj(actor_id, &key, "https://ex.example/users/alice/inbox"),
        "object": {
            "id": note_id,
            "type": "Note",
            "attributedTo": actor_id,
            "content": "hello from mastodon"
        }
    });
    let body = serde_json::to_vec(&activity).unwrap();
    let host = format!("{}", knot.addr);
    let url = format!("http://{host}/inbox");
    let resp = post_signed(
        &url,
        "/inbox",
        &host,
        &key,
        &format!("{actor_id}#main-key"),
        &body,
    )
    .await;
    assert_eq!(resp.status(), 202, "{}", resp.text().await.unwrap());

    let did = activitypub_actor_identity(actor_id).did_string();
    let rkey = import_rkey("activitypub", note_id).unwrap();
    let foreign = knot
        .client()
        .get_record(&did, "sys.foreign", &rkey)
        .await
        .unwrap();
    assert_eq!(foreign.record["body"]["attested"], true);
    assert_eq!(foreign.record["body"]["protocol"], "activitypub");
    assert_eq!(foreign.record["body"]["foreign_id"], note_id);
    assert_eq!(foreign.record["body"]["acct"], "alice@ex.example");
    assert_eq!(foreign.record["body"]["foreign"], actor_id);
    let profile = knot
        .client()
        .get_record(&did, "pub.reed.profile", "self")
        .await
        .unwrap();
    assert_eq!(profile.record["body"]["handle"], "alice@ex.example");
    assert_eq!(profile.record["body"]["url"], actor_id);
    assert_eq!(profile.record["body"]["displayName"], "Alice");
    assert_eq!(profile.record["body"]["bio"], "Writes notes");
    let post = knot
        .client()
        .get_record(&did, "pub.tangled.post", &rkey)
        .await
        .unwrap();
    assert_eq!(post.record["body"]["text"], "hello from mastodon");

    let repo = knot.client().get_repo(&did, None, None).await.unwrap();
    let cols = collections_in_repo(&repo);
    assert!(
        cols.iter().any(|c| c == "pub.tangled.post"),
        "getRepo missing post: {cols:?}"
    );
    assert!(
        cols.iter().any(|c| c == "sys.foreign"),
        "getRepo missing sys.foreign: {cols:?}"
    );
}

#[tokio::test]
async fn inbox_follow_like_announce() {
    let knot = Live::named("ap-social").await;
    let key = rsa();
    let actor_id = "https://ex.example/users/bob";
    seed(&knot, actor_id, &key, "https://ex.example/inbox");
    let note_id = "https://ex.example/users/bob/statuses/9";
    let create = serde_json::json!({
        "id": "https://ex.example/users/bob/statuses/9/activity",
        "type": "Create",
        "actor": actor_obj(actor_id, &key, "https://ex.example/inbox"),
        "object": {
            "id": note_id,
            "type": "Note",
            "content": "boost me"
        }
    });
    let host = format!("{}", knot.addr);
    let inbox = format!("http://{host}/inbox");
    let key_id = format!("{actor_id}#main-key");
    let resp = post_signed(
        &inbox,
        "/inbox",
        &host,
        &key,
        &key_id,
        &serde_json::to_vec(&create).unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 202);

    let follow = follow_activity(
        "https://ex.example/users/bob/follow/1",
        actor_id,
        &format!("http://{host}/users/gateway"),
    );
    let mut follow = follow;
    follow["actor"] = actor_obj(actor_id, &key, "https://ex.example/inbox");
    let resp = post_signed(
        &inbox,
        "/inbox",
        &host,
        &key,
        &key_id,
        &serde_json::to_vec(&follow).unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 202, "follow");

    let like = like_activity("https://ex.example/users/bob/likes/1", actor_id, note_id);
    let mut like = like;
    like["actor"] = actor_obj(actor_id, &key, "https://ex.example/inbox");
    let resp = post_signed(
        &inbox,
        "/inbox",
        &host,
        &key,
        &key_id,
        &serde_json::to_vec(&like).unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 202, "like");

    let ann = announce_activity("https://ex.example/users/bob/boosts/1", actor_id, note_id);
    let mut ann = ann;
    ann["actor"] = actor_obj(actor_id, &key, "https://ex.example/inbox");
    let resp = post_signed(
        &inbox,
        "/inbox",
        &host,
        &key,
        &key_id,
        &serde_json::to_vec(&ann).unwrap(),
    )
    .await;
    assert_eq!(resp.status(), 202, "announce");

    let did = activitypub_actor_identity(actor_id).did_string();
    let follow_rkey = tangled::sync::safe_did_rkey(&knot.did());
    let got = knot
        .client()
        .get_record(&did, "tangled.graph.follow", &follow_rkey)
        .await
        .unwrap();
    assert_eq!(got.record["body"]["subject"], knot.did());

    let like_rkey = import_rkey("activitypub", "https://ex.example/users/bob/likes/1").unwrap();
    let liked = knot
        .client()
        .get_record(&did, "pub.reed.like", &like_rkey)
        .await
        .unwrap();
    assert!(liked.record["body"]["target"].as_str().is_some());

    let boost_rkey = import_rkey("activitypub", "https://ex.example/users/bob/boosts/1").unwrap();
    knot.client()
        .get_record(&did, "pub.reed.boost", &boost_rkey)
        .await
        .unwrap();

    let like_env = knot
        .client()
        .get_record(&did, "sys.foreign", &like_rkey)
        .await
        .unwrap();
    assert_eq!(like_env.record["body"]["attested"], true);
}

#[tokio::test]
async fn local_put_emits_create_note() {
    let alice = Live::named("ap-out").await;
    let _ = identity_named("ap-out");
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "hello",
            "public",
            serde_json::json!({ "text": "outbound" }),
        ))
        .await
        .unwrap();
    // Delivery list is process-local; outbox GET still maps public posts.
    let host = format!("{}", alice.addr);
    let name =
        alice.did().strip_prefix("did:sol:").unwrap()[..32.min(alice.did().len())].to_string();
    // gateway outbox lists all public posts
    let outbox: serde_json::Value = reqwest::get(format!("http://{host}/users/gateway/outbox"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let items = outbox["orderedItems"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        items.iter().any(|n| n["content"] == "outbound"),
        "outbox: {outbox}"
    );
    let _ = name;
}

fn create_body(actor_id: &str) -> Vec<u8> {
    let activity = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{actor_id}/statuses/1/activity"),
        "type": "Create",
        "actor": actor_id,
        "object": {
            "id": format!("{actor_id}/statuses/1"),
            "type": "Note",
            "attributedTo": actor_id,
            "content": "hello"
        }
    });
    serde_json::to_vec(&activity).unwrap()
}

#[tokio::test]
async fn inbox_rejects_missing_signature_with_seeded_actor() {
    let knot = Live::named("ap-nosig").await;
    let key = rsa();
    let actor_id = "https://ex.example/users/nosig";
    seed(
        &knot,
        actor_id,
        &key,
        "https://ex.example/users/nosig/inbox",
    );
    let host = format!("{}", knot.addr);
    let body = create_body(actor_id);
    let resp = reqwest::Client::new()
        .post(format!("http://{host}/inbox"))
        .header("Host", &host)
        .header("Date", http_date(unix_now()))
        .header("Digest", digest_header(&body))
        .header("Content-Type", "application/activity+json")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn inbox_rejects_wrong_signature_with_seeded_actor() {
    let knot = Live::named("ap-wrongsig").await;
    let key = rsa();
    let other = rsa();
    let actor_id = "https://ex.example/users/wrong";
    seed(
        &knot,
        actor_id,
        &key,
        "https://ex.example/users/wrong/inbox",
    );
    let host = format!("{}", knot.addr);
    let body = create_body(actor_id);
    let resp = post_signed(
        &format!("http://{host}/inbox"),
        "/inbox",
        &host,
        &other,
        &format!("{actor_id}#main-key"),
        &body,
    )
    .await;
    assert_eq!(resp.status(), 401, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn inbox_rejects_signature_omitting_digest() {
    let knot = Live::named("ap-nodigest").await;
    let key = rsa();
    let actor_id = "https://ex.example/users/nodigest";
    seed(
        &knot,
        actor_id,
        &key,
        "https://ex.example/users/nodigest/inbox",
    );
    let host = format!("{}", knot.addr);
    let body = create_body(actor_id);
    let date = http_date(unix_now());
    let digest = digest_header(&body);
    let headers = vec![("host".into(), host.clone()), ("date".into(), date.clone())];
    let signature = sign_header_list(
        &key,
        &format!("{actor_id}#main-key"),
        "POST",
        "/inbox",
        "(request-target) host date",
        &headers,
    )
    .unwrap();
    let resp = reqwest::Client::new()
        .post(format!("http://{host}/inbox"))
        .header("Host", &host)
        .header("Date", date)
        .header("Digest", digest)
        .header("Signature", signature)
        .header("Content-Type", "application/activity+json")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn inbox_rejects_stale_date() {
    let knot = Live::named("ap-olddate").await;
    let key = rsa();
    let actor_id = "https://ex.example/users/olddate";
    seed(
        &knot,
        actor_id,
        &key,
        "https://ex.example/users/olddate/inbox",
    );
    let host = format!("{}", knot.addr);
    let body = create_body(actor_id);
    let date = http_date(1_700_000_000);
    let (signature, digest) = sign_request(
        &key,
        &format!("{actor_id}#main-key"),
        "POST",
        "/inbox",
        &host,
        &date,
        &body,
    );
    let resp = reqwest::Client::new()
        .post(format!("http://{host}/inbox"))
        .header("Host", &host)
        .header("Date", date)
        .header("Digest", digest)
        .header("Signature", signature)
        .header("Content-Type", "application/activity+json")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "{}", resp.text().await.unwrap());
}

#[tokio::test]
async fn inbox_rejects_embedded_pem_not_matching_fetched_actor() {
    let knot = Live::named("ap-embedpem").await;
    let real = rsa();
    let attacker = rsa();
    let actor_id = "https://ex.example/users/real";
    seed(
        &knot,
        actor_id,
        &real,
        "https://ex.example/users/real/inbox",
    );
    let activity = serde_json::json!({
        "id": "https://ex.example/users/real/statuses/1/activity",
        "type": "Create",
        "actor": actor_obj(actor_id, &attacker, "https://ex.example/users/real/inbox"),
        "object": {
            "id": "https://ex.example/users/real/statuses/1",
            "type": "Note",
            "content": "forged"
        }
    });
    let body = serde_json::to_vec(&activity).unwrap();
    let host = format!("{}", knot.addr);
    let resp = post_signed(
        &format!("http://{host}/inbox"),
        "/inbox",
        &host,
        &attacker,
        &format!("{actor_id}#main-key"),
        &body,
    )
    .await;
    assert_eq!(
        resp.status(),
        401,
        "embedded attacker PEM must not authenticate: {}",
        resp.text().await.unwrap()
    );
}

#[tokio::test]
async fn remember_actor_rejects_metadata_inbox() {
    let knot = Live::named("ap-ssrf").await;
    let key = rsa();
    let actor_id = "https://ex.example/users/ssrf";
    let mut evil = actor_obj(actor_id, &key, "http://169.254.169.254/latest");
    evil["endpoints"] = serde_json::json!({"sharedInbox": "http://169.254.169.254/latest"});
    knot.seed_actor(evil.clone());
    let activity = serde_json::json!({
        "id": "https://ex.example/users/ssrf/follow/1",
        "type": "Follow",
        "actor": evil,
        "object": format!("http://{}/users/gateway", knot.addr)
    });
    let body = serde_json::to_vec(&activity).unwrap();
    let host = format!("{}", knot.addr);
    let resp = post_signed(
        &format!("http://{host}/inbox"),
        "/inbox",
        &host,
        &key,
        &format!("{actor_id}#main-key"),
        &body,
    )
    .await;
    assert_eq!(resp.status(), 202, "{}", resp.text().await.unwrap());
    assert!(
        knot.remote_inbox(actor_id).is_none(),
        "metadata inbox must not be stored: {:?}",
        knot.remote_inbox(actor_id)
    );
}

/// Demo helper: mock Mastodon peer on loopback, fetched-key inbox (no `seed_actor`).
#[tokio::test]
async fn demo_peer_create_note_ingests_via_fetched_key() {
    let knot = Live::named("ap-demo-peer").await;
    let peer = tangled_ap::demo::MockPeer::bind("demo").await.unwrap();
    let host = format!("{}", knot.addr);
    let inbox = format!("http://{host}/inbox");
    let delivered = peer
        .post_create_note(&inbox, "hello from the mock mastodon")
        .await
        .unwrap();
    assert_eq!(delivered.status, 202, "{}", delivered.body);

    let did = activitypub_actor_identity(peer.actor_id()).did_string();
    let rkey = import_rkey("activitypub", &delivered.note_id).unwrap();
    let foreign = knot
        .client()
        .get_record(&did, "sys.foreign", &rkey)
        .await
        .unwrap();
    assert_eq!(foreign.record["body"]["attested"], true);
    assert_eq!(foreign.record["body"]["protocol"], "activitypub");
    let post = knot
        .client()
        .get_record(&did, "pub.tangled.post", &rkey)
        .await
        .unwrap();
    assert_eq!(post.record["body"]["text"], "hello from the mock mastodon");

    let resolved = knot.client().ap_resolve(&peer.acct()).await.unwrap();
    assert_eq!(resolved.did, did);
    assert_eq!(resolved.actor, peer.actor_id());
}

/// `tangled.ap.resolve` WebFinger + actor + public outbox GET (no HTTP sig).
#[tokio::test]
async fn resolve_pulls_outbox_create_note() {
    let knot = Live::named("ap-outbox-pull").await;
    let peer = tangled_ap::demo::MockPeer::bind("alice").await.unwrap();
    let note_id = peer.publish_note("pulled from the public outbox");
    let resolved = knot.client().ap_resolve(&peer.acct()).await.unwrap();
    assert_eq!(resolved.actor, peer.actor_id());

    let did = activitypub_actor_identity(peer.actor_id()).did_string();
    assert_eq!(resolved.did, did);
    let rkey = import_rkey("activitypub", &note_id).unwrap();
    let foreign = knot
        .client()
        .get_record(&did, "sys.foreign", &rkey)
        .await
        .unwrap();
    assert_eq!(foreign.record["body"]["attested"], true);
    assert_eq!(foreign.record["body"]["protocol"], "activitypub");
    let post = knot
        .client()
        .get_record(&did, "pub.tangled.post", &rkey)
        .await
        .unwrap();
    assert_eq!(post.record["body"]["text"], "pulled from the public outbox");
}

/// Resolve persists display name, HTML-stripped bio, handle, and fetched avatar blob.
#[tokio::test]
async fn resolve_writes_full_profile_with_avatar() {
    let knot = Live::named("ap-profile-avatar").await;
    let peer = tangled_ap::demo::MockPeer::bind("gargron").await.unwrap();
    let resolved = knot.client().ap_resolve(&peer.acct()).await.unwrap();
    let did = activitypub_actor_identity(peer.actor_id()).did_string();
    assert_eq!(resolved.did, did);

    let profile = knot
        .client()
        .get_record(&did, "pub.reed.profile", "self")
        .await
        .unwrap();
    assert_eq!(profile.record["body"]["displayName"], "Demo");
    assert_eq!(profile.record["body"]["bio"], "Local mock Mastodon peer");
    assert!(
        profile.record["body"]["handle"]
            .as_str()
            .is_some_and(|h| h.starts_with("gargron@")),
        "handle: {}",
        profile.record["body"]["handle"]
    );
    assert_eq!(profile.record["body"]["url"], peer.actor_id());
    let cid = profile.record["body"]["avatar"]
        .as_str()
        .expect("avatar cid");
    let blob = knot.client().get_blob(&did, cid).await.unwrap();
    assert_eq!(blob, tangled_ap::demo::TINY_PNG);
}

/// Live Mastodon outbox. CI never sets this; no required public internet.
#[tokio::test]
#[ignore]
async fn live_mastodon_outbox_pull() {
    if std::env::var("REED_AP_LIVE").ok().as_deref() != Some("1") {
        return;
    }
    let knot = Live::named("ap-live-outbox").await;
    let acct =
        std::env::var("REED_AP_ACCT").unwrap_or_else(|_| "acct:Gargron@mastodon.social".into());
    let resolved = knot
        .client()
        .ap_resolve(&acct)
        .await
        .expect("resolve live Mastodon acct");
    assert!(
        resolved.actor.starts_with("https://"),
        "actor: {}",
        resolved.actor
    );
    let repo = knot
        .client()
        .get_repo(&resolved.did, None, None)
        .await
        .expect("getRepo after outbox pull");
    let cols = collections_in_repo(&repo);
    assert!(
        cols.iter().any(|c| c == "pub.tangled.post"),
        "live outbox should ingest at least one Note: {cols:?}"
    );
}
