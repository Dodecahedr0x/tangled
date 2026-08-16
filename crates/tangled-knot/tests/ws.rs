//! WebSocket subscribe / firehose round-trip.

mod common;

use common::Live;
use futures_util::{SinkExt, StreamExt};
use tangled::http::SubscribeMessage;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn subscribe_round_trip() {
    let alice = Live::named("ws-alice").await;
    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "seed",
            "public",
            serde_json::json!({ "text": "seed" }),
        ))
        .await
        .unwrap();

    let url = format!(
        "ws://{}/xrpc/tangled.sync.subscribe?did={}",
        alice.addr,
        urlencoding_lite(&alice.did())
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    let hello = recv_json(&mut ws).await;
    assert_eq!(hello.t, "commit");
    assert_eq!(hello.did, alice.did());

    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "live",
            "public",
            serde_json::json!({ "text": "live" }),
        ))
        .await
        .unwrap();

    let ev = recv_json(&mut ws).await;
    assert_eq!(ev.t, "commit");
    assert!(ev.ops.iter().any(|op| op.rkey == "live"));
    ws.close(None).await.ok();
}

#[tokio::test]
async fn subscribe_since_replays_later_commits() {
    let alice = Live::named("ws-since").await;
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
    let url = format!(
        "ws://{}/xrpc/tangled.sync.subscribe?did={}&since={}",
        alice.addr,
        urlencoding_lite(&alice.did()),
        urlencoding_lite(&since)
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");
    let ev = recv_json(&mut ws).await;
    assert_eq!(ev.t, "commit");
    assert!(ev.ops.iter().any(|op| op.rkey == "b"));
    ws.close(None).await.ok();
}

#[tokio::test]
async fn firehose_public_commit() {
    let alice = Live::named("fh-alice").await;
    let url = format!("ws://{}/xrpc/tangled.loom.firehose", alice.addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("firehose connect");

    alice
        .publish(alice.unsigned(
            "pub.tangled.post",
            "fh1",
            "public",
            serde_json::json!({ "text": "hose" }),
        ))
        .await
        .unwrap();

    let ev = recv_json(&mut ws).await;
    assert!(ev.ops.iter().any(|op| op.rkey == "fh1"));
    ws.close(None).await.ok();
}

#[tokio::test]
async fn subscribe_sealed_collection_requires_cap() {
    let alice = Live::named("ws-sealed").await;
    let mut chat = alice.unsigned(
        "pub.tangled.chat",
        "s1",
        "sealed",
        serde_json::json!({ "ciphertext": "aa" }),
    );
    chat["circle"] = serde_json::json!(format!("{}#c", alice.did()));
    alice.publish(chat).await.unwrap();

    let url = format!(
        "ws://{}/xrpc/tangled.sync.subscribe?did={}&collection=pub.tangled.chat",
        alice.addr,
        urlencoding_lite(&alice.did())
    );
    let err = tokio_tungstenite::connect_async(&url).await;
    assert!(err.is_err());
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
