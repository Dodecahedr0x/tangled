use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tangled_provider::{router, Provider};
use tempfile::tempdir;
use tower::ServiceExt;

async fn body_string(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn cookie(response: &axum::response::Response) -> String {
    let raw = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn signup_posts_and_knot_discloses_custody() {
    let dir = tempdir().unwrap();
    let provider = Provider::open(dir.path(), "http://127.0.0.1:9").unwrap();
    let app = router(Arc::clone(&provider));

    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let signup = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/signup")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=ada@example.com&name=Ada&password=correct-horse",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(signup.status(), StatusCode::SEE_OTHER);
    let session = cookie(&signup);

    let home = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = body_string(home).await;
    assert!(html.contains("Ada"), "{html}");
    assert!(html.contains("did:sol:"), "{html}");

    let posted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/posts")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &session)
                .body(Body::from("text=hello+from+the+host"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::SEE_OTHER);

    let again = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = body_string(again).await;
    assert!(html.contains("hello from the host"), "{html}");

    let guest = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let html = body_string(guest).await;
    assert!(html.contains("Create an account"), "{html}");
    assert!(html.contains("hello from the host"), "{html}");

    let describe = app
        .oneshot(
            Request::builder()
                .uri("/xrpc/tangled.node.describe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_string(describe).await;
    assert!(
        json.contains("\"custodialSign\":true") || json.contains("\"custodialSign\": true"),
        "{json}"
    );
}
