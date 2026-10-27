use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::ArchiveStore;
use tangled_knot::imports::{router, ImportState};
use tower::ServiceExt;

async fn json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}
fn request(method: &str, uri: &str, body: impl Into<Body>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", "owner=yes")
        .header("content-type", "application/json")
        .body(body.into())
        .unwrap()
}

#[tokio::test]
async fn browser_chunks_resume_and_erasure_removes_retained_data_and_fences_replay() {
    let dir = tempfile::tempdir().unwrap();
    let id = Identity::from_seeds([111; 32], [112; 32]);
    let owner = id.did_string();
    let mut engine = KnotEngine::hosting(&owner);
    engine.register_identity(&id);
    let state = ImportState {
        archive: Arc::new(Mutex::new(ArchiveStore::open(dir.path()).unwrap())),
        engine: Arc::new(Mutex::new(engine)),
        authenticate: Arc::new(move |headers| {
            (headers.get("cookie").and_then(|h| h.to_str().ok()) == Some("owner=yes"))
                .then(|| (owner.clone(), Identity::from_seeds([111; 32], [112; 32])))
        }),
        custodial: false,
    };
    let app = router(state.clone());
    let raw=br#"{"id":77,"name":"Chat","messages":[{"id":1,"type":"message","from":"Other","date":"2024-01-01T12:00:00","text":"erase this content"}]}"#;
    let start = serde_json::json!({"source":"telegram","account":"123","filename":"chat.json","total":raw.len(),"sha256":hex::encode(Sha256::digest(raw))});
    let response = app
        .clone()
        .oneshot(request("POST", "/api/imports/uploads", start.to_string()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let upload = json(response).await;
    let id = upload["id"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(request(
            "PUT",
            &format!("/api/imports/uploads/{id}/chunks?offset=0"),
            raw[..20].to_vec(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    for (offset, chunk) in [(0, &raw[..20]), (20, &raw[20..])] {
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/api/imports/uploads/{id}/chunks?offset={offset}"))
            .header("cookie", "owner=yes")
            .header("x-chunk-sha256", hex::encode(Sha256::digest(chunk)))
            .body(Body::from(chunk.to_vec()))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    let status = json(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/imports/uploads/{id}"),
                Body::empty(),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status["status"], "ready");
    let complete_uri = format!("/api/imports/uploads/{id}/complete");
    let complete = json(
        app.clone()
            .oneshot(request("POST", &complete_uri, Body::empty()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(complete["status"], "complete");
    assert!(complete["receipt"]["inserted"].as_u64().unwrap() > 0);
    let again = json(
        app.clone()
            .oneshot(request("POST", &complete_uri, Body::empty()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(again["receipt"]["id"], complete["receipt"]["id"]);
    let response = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/imports/erase",
            r#"{"confirmation":"no"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let erase = app
        .clone()
        .oneshot(request(
            "POST",
            "/api/imports/erase",
            r#"{"confirmation":"ERASE"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(erase.status(), StatusCode::OK, "{}", json(erase).await);
    assert!(state
        .archive
        .lock()
        .unwrap()
        .objects(
            &Identity::from_seeds([111; 32], [112; 32]).did_string(),
            &Identity::from_seeds([111; 32], [112; 32])
        )
        .unwrap()
        .is_empty());
    let response = app
        .oneshot(request("POST", &complete_uri, Body::empty()))
        .await
        .unwrap();
    assert_ne!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn an_old_streaming_upload_cannot_recreate_data_after_erasure() {
    let dir = tempfile::tempdir().unwrap();
    let id = Identity::from_seeds([113; 32], [114; 32]);
    let owner = id.did_string();
    let mut engine = KnotEngine::hosting(&owner);
    engine.register_identity(&id);
    let app = router(ImportState {
        archive: Arc::new(Mutex::new(ArchiveStore::open(dir.path()).unwrap())),
        engine: Arc::new(Mutex::new(engine)),
        authenticate: Arc::new(move |headers| {
            (headers.get("cookie").and_then(|h| h.to_str().ok()) == Some("owner=yes"))
                .then(|| (owner.clone(), Identity::from_seeds([113; 32], [114; 32])))
        }),
        custodial: false,
    });
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let raw=br#"{"id":1,"name":"Chat","messages":[{"id":1,"type":"message","text":"must not return"}]}"#;
    let payload=serde_json::json!({"source":"telegram","account":"123","filename":"chat.json","data_base64":STANDARD.encode(raw)}).to_string().into_bytes();
    let (sender, receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    let polled = Arc::new(tokio::sync::Notify::new());
    let notify = polled.clone();
    let stream =
        futures_util::stream::unfold((receiver, notify), |(mut receiver, notify)| async move {
            receiver.recv().await.map(|bytes| {
                notify.notify_one();
                (Ok::<Vec<u8>, std::io::Error>(bytes), (receiver, notify))
            })
        });
    sender.send(payload[..20].to_vec()).await.unwrap();
    let old_app = app.clone();
    let upload = tokio::spawn(async move {
        old_app
            .oneshot(request(
                "POST",
                "/api/imports/upload",
                Body::from_stream(stream),
            ))
            .await
            .unwrap()
    });
    polled.notified().await;
    let erased = app
        .oneshot(request(
            "POST",
            "/api/imports/erase",
            r#"{"confirmation":"ERASE"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(erased.status(), StatusCode::OK);
    sender.send(payload[20..].to_vec()).await.unwrap();
    drop(sender);
    let stale = upload.await.unwrap();
    assert_ne!(
        stale.status(),
        StatusCode::OK,
        "a request begun before erasure must not be treated as a new authorized import"
    );
}
