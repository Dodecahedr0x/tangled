//! Filesystem snapshot: restart the engine from a data dir.

mod common;

use common::{http_code, identity_named, Live, NOW};
use futures_util::{SinkExt, StreamExt};
use tangled::http::{KnotClient, SubscribeMessage};
use tangled::KnotEngine;
use tangled_knot::{bind_ephemeral, load_or_create_identity};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn persist_restart_keeps_records() {
    let dir = tempfile::tempdir().unwrap();
    let alice = Live::with_data("fs-alice", dir.path().to_path_buf()).await;
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "kept",
            "public",
            serde_json::json!({ "text": "durable" }),
        ))
        .await
        .unwrap();
    let blob = alice
        .client()
        .put_blob(b"blob-bytes".to_vec())
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.media",
            "m1",
            "public",
            serde_json::json!({ "blob": blob.cid, "alt": "file" }),
        ))
        .await
        .unwrap();

    let did = alice.did();
    drop(alice);

    let mut engine = KnotEngine::open(dir.path(), &did).unwrap();
    engine.now = NOW;
    let (addr, _) = bind_ephemeral(engine, false).await.unwrap();
    let client = KnotClient::http(format!("http://{addr}"));
    let got = client
        .get_record(&did, "pub.tangled.post", "kept")
        .await
        .unwrap();
    assert_eq!(got.record["body"]["text"], "durable");
    let bytes = client.get_blob(&did, &blob.cid).await.unwrap();
    assert_eq!(bytes, b"blob-bytes");
}

#[tokio::test]
async fn identity_file_roundtrip_and_handle_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let a = load_or_create_identity(dir.path()).unwrap();
    let b = load_or_create_identity(dir.path()).unwrap();
    assert_eq!(a.did_string(), b.did_string());

    let named = identity_named("handle-alice");
    let mut engine = KnotEngine::hosting(&named.did_string());
    engine.now = NOW;
    engine.handle_resolver.txt.insert(
        tangled::handle::txt_name("alice.test"),
        vec![named.did_string()],
    );
    let rec = serde_json::json!({
        "v": 1,
        "collection": "sys.id",
        "rkey": "self",
        "author": named.did_string(),
        "vis": "public",
        "body": { "alsoKnownAs": ["alice.test"] },
    });
    engine.stored.push(tangled::knot::StoredRecord {
        did: named.did_string(),
        collection: "sys.id".into(),
        rkey: "self".into(),
        vis: "public".into(),
        cid: "bafyid".into(),
        record: rec,
        circle: None,
        deleted: false,
    });
    let (addr, _) = bind_ephemeral(engine, false).await.unwrap();
    let client = KnotClient::http(format!("http://{addr}"));
    let resolved = client
        .identity_resolve(Some("alice.test"), None)
        .await
        .unwrap();
    assert_eq!(resolved.did, named.did_string());
}

#[tokio::test]
async fn sns_handle_resolve_http() {
    let named = identity_named("sns-alice");
    let mut engine = KnotEngine::hosting(&named.did_string());
    engine.now = NOW;
    engine.handle_resolver.sns.insert(
        "alice.sol".into(),
        tangled::handle::SnsRecord {
            owner: named.pubkey_bytes(),
            tangled: Some(named.did_string()),
            head: None,
        },
    );
    let rec = serde_json::json!({
        "v": 1,
        "collection": "sys.id",
        "rkey": "self",
        "author": named.did_string(),
        "vis": "public",
        "body": { "alsoKnownAs": ["alice.sol"] },
    });
    engine.stored.push(tangled::knot::StoredRecord {
        did: named.did_string(),
        collection: "sys.id".into(),
        rkey: "self".into(),
        vis: "public".into(),
        cid: "bafyid".into(),
        record: rec,
        circle: None,
        deleted: false,
    });
    let (addr, _) = bind_ephemeral(engine, false).await.unwrap();
    let client = KnotClient::http(format!("http://{addr}"));
    let resolved = client
        .identity_resolve(Some("alice.sol"), None)
        .await
        .unwrap();
    assert_eq!(resolved.did, named.did_string());
}

#[tokio::test]
async fn max_uses_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let alice = Live::with_data("uses-fs-alice", dir.path().to_path_buf()).await;
    let bob = Live::named("uses-fs-bob").await;
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
    let did = alice.did();
    drop(alice);

    let mut engine = KnotEngine::open(dir.path(), &did).unwrap();
    engine.now = NOW;
    let (addr, _) = bind_ephemeral(engine, false).await.unwrap();
    let err = KnotClient::http(format!("http://{addr}"))
        .with_cap_header(header)
        .get_record(&did, "pub.tangled.post", "x")
        .await
        .unwrap_err();
    assert_eq!(http_code(&err).0, 403);
}

#[tokio::test]
async fn subscribe_since_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let alice = Live::with_data("ws-fs-alice", dir.path().to_path_buf()).await;
    let first = alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "a",
            "public",
            serde_json::json!({ "text": "a" }),
        ))
        .await
        .unwrap();
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "b",
            "public",
            serde_json::json!({ "text": "b" }),
        ))
        .await
        .unwrap();
    let since = first.put.commit.clone().expect("commit");
    let did = alice.did();
    drop(alice);

    let mut engine = KnotEngine::open(dir.path(), &did).unwrap();
    engine.now = NOW;
    let (addr, _) = bind_ephemeral(engine, false).await.unwrap();
    let url = format!(
        "ws://{}/xrpc/tangled.sync.subscribe?did={}&since={}",
        addr,
        urlencoding_lite(&did),
        urlencoding_lite(&since)
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");
    let ev = recv_json(&mut ws).await;
    assert!(ev.ops.iter().any(|op| op.rkey == "b"));
    ws.close(None).await.ok();
}

async fn recv_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> SubscribeMessage {
    for _ in 0..20 {
        match tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                return serde_json::from_str(&t).expect("subscribe json");
            }
            Ok(Some(Ok(Message::Ping(p)))) => {
                let _ = ws.send(Message::Pong(p)).await;
            }
            Ok(Some(Ok(_))) => continue,
            other => panic!("expected ws text, got {other:?}"),
        }
    }
    panic!("no subscribe message");
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
