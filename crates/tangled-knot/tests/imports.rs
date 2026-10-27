use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use http_body_util::BodyExt;
use std::sync::{Arc, Mutex};
use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::ArchiveStore;
use tangled_knot::imports::{router, ImportState};
use tower::ServiceExt;

#[tokio::test]
async fn private_archive_upload_download_and_revoked_connection() {
    let dir = tempfile::tempdir().unwrap();
    let id = Identity::from_seeds([11; 32], [12; 32]);
    let owner = id.did_string();
    let mut engine = KnotEngine::hosting(&owner);
    engine.register_identity(&id);
    let state = ImportState {
        archive: Arc::new(Mutex::new(ArchiveStore::open(dir.path()).unwrap())),
        engine: Arc::new(Mutex::new(engine)),
        authenticate: Arc::new(move |headers| {
            (headers.get("cookie").and_then(|h| h.to_str().ok()) == Some("session=test"))
                .then(|| (owner.clone(), Identity::from_seeds([11; 32], [12; 32])))
        }),
        custodial: false,
    };
    let app = router(state.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/imports")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let raw=br#"{"name":"Alice","id":77,"messages":[{"id":1,"type":"message","date":"2024-01-01T12:00:00","from":"Bob","text":"private hello"}]}"#;
    let payload = serde_json::json!({"source":"telegram","account":"123","filename":"result.json","data_base64":STANDARD.encode(raw)});
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/imports/upload")
                .header("cookie", "session=test")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let receipt: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(status, StatusCode::OK, "{receipt}");
    assert!(receipt["inserted"].as_u64().unwrap() > 0);
    let uri = format!("/api/imports/original/{}", receipt["id"].as_str().unwrap());
    let response = app
        .clone()
        .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header("cookie", "session=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        &response.into_body().collect().await.unwrap().to_bytes()[..],
        raw
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/imports/export")
                .header("cookie", "session=test")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"passphrase":"separate archive recovery passphrase"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let packet: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let recovered =
        tangled_import::recovery_open(&packet, "separate archive recovery passphrase").unwrap();
    assert_eq!(recovered.owner, id.did_string());
    assert_eq!(recovered.identity().unwrap().to_seeds(), id.to_seeds());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/imports/export")
                .header("cookie", "session=test")
                .header("origin", "https://untrusted.example")
                .header("host", "127.0.0.1")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"passphrase":"separate archive recovery passphrase"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/imports")
                .header("cookie", "session=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/imports/connections")
                .header("cookie", "session=test")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"source":"telegram","account":"123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let connection: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let connection_id = connection["connection"]["id"].as_str().unwrap();
    let token = connection["token"].as_str().unwrap();
    state
        .archive
        .lock()
        .unwrap()
        .disconnect(&id.did_string(), connection_id)
        .unwrap();
    let response=app.oneshot(Request::builder().method("POST").uri(format!("/api/imports/connections/{connection_id}/ingest")).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(r#"{"source":"telegram","account":"123","objects":[],"files":[],"warnings":[]}"#)).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn self_hosted_archive_requires_the_local_identity_session() {
    let dir = tempfile::tempdir().unwrap();
    let id = tangled_knot::load_or_create_identity(dir.path()).unwrap();
    let mut engine = KnotEngine::open(dir.path(), &id.did_string()).unwrap();
    engine.register_identity(&id);
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    engine.sessions.insert(
        "owner".into(),
        tangled::session::SessionRecord {
            did: id.did_string(),
            exp,
        },
    );
    engine.sessions.insert(
        "other".into(),
        tangled::session::SessionRecord {
            did: Identity::from_seeds([13; 32], [14; 32]).did_string(),
            exp,
        },
    );
    let app = tangled_knot::router(tangled_knot::AppState::new(engine, false));
    for (token, want) in [
        ("owner", StatusCode::OK),
        ("other", StatusCode::UNAUTHORIZED),
        ("unknown", StatusCode::UNAUTHORIZED),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/imports")
                    .header(
                        "cookie",
                        format!("{}={token}", tangled::session::COOKIE_NAME),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), want);
    }
}
