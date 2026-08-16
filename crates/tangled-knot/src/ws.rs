//! WebSocket `subscribe` / `loom.firehose` (OpenAPI 101). Public commits by default;
//! non-public collection subscribe requires a capability (spec 11).

use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tangled::http::{ApiError, KnotRequest, Method};

use crate::{socket_loop, AppState, SubscribeQuery};

pub async fn subscribe(
    ws: WebSocketUpgrade,
    Query(q): Query<SubscribeQuery>,
    headers: HeaderMap,
    State(st): State<AppState>,
) -> Response {
    if q.did.is_empty() {
        return (StatusCode::BAD_REQUEST, "did required").into_response();
    }
    if let Some(col) = &q.collection {
        let mut engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        let mut hdrs = Vec::new();
        for (k, v) in &headers {
            if let Ok(val) = v.to_str() {
                hdrs.push((k.as_str().to_string(), val.to_string()));
            }
        }
        let req = KnotRequest {
            method: Method::Get,
            path: "/xrpc/tangled.sync.subscribe".into(),
            query: vec![
                ("did".into(), q.did.clone()),
                ("collection".into(), col.clone()),
            ],
            headers: hdrs,
            body: None,
            content_type: None,
        };
        if let Err(e) = engine.subscribe_authorized(&q.did, col, &req) {
            drop(engine);
            return cap_error(e);
        }
    }
    let did = q.did;
    let collection = q.collection;
    let since = q.since;
    ws.on_upgrade(move |socket| socket_loop(socket, st, Some(did), collection, since))
}

pub async fn firehose(
    ws: WebSocketUpgrade,
    Query(q): Query<FirehoseQuery>,
    State(st): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| socket_loop(socket, st, None, q.collection, None))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct FirehoseQuery {
    pub collection: Option<String>,
}

fn cap_error(err: tangled::Error) -> Response {
    let (status, error, message) = match err {
        tangled::Error::Http { status, error } => {
            let message = error.clone();
            (status, error, message)
        }
        other => (500, "Unavailable".into(), other.to_string()),
    };
    let body = serde_json::to_vec(&ApiError {
        error,
        message,
        retry: false,
        head: None,
    })
    .unwrap_or_else(|_| b"{\"error\":\"Unavailable\"}".to_vec());
    Response::builder()
        .status(status)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
