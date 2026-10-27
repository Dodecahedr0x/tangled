//! Local mock Mastodon/ActivityPub peer for `just reed-ap-demo-mock`.
//!
//! Serves WebFinger + actor + outbox (fetched-key path) and POSTs signed
//! `Create(Note)` to a knot inbox. CI never needs the public internet.
//!
//! The default demo (`just reed-ap-demo`) follows a real Mastodon account and
//! pulls its public outbox. Use this mock when you want a loopback peer.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rand::rngs::OsRng;
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Deserialize;

use crate::httpsig::{http_date, sign_request, unix_now};
use crate::note::{create_note, post_to_note};
use crate::webfinger::build_jrd;
use crate::{person_document, Error, Result};

const AP_JSON: &str = "application/activity+json";
const JRD: &str = "application/jrd+json";

/// 1×1 PNG so resolve tests can fetch a real avatar without the public internet.
pub const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE, 0xD4, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[derive(Clone)]
struct PeerState {
    username: String,
    actor_id: String,
    pem: String,
    notes: Arc<Mutex<Vec<(String, String)>>>,
}

/// Loopback ActivityPub actor that the local knot can fetch.
pub struct MockPeer {
    pub addr: SocketAddr,
    username: String,
    actor_id: String,
    key: RsaPrivateKey,
    seq: AtomicU64,
    notes: Arc<Mutex<Vec<(String, String)>>>,
    server_task: tokio::task::JoinHandle<()>,
}

impl Drop for MockPeer {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

/// Result of a signed inbox `Create(Note)`.
pub struct DeliveredNote {
    pub status: u16,
    pub body: String,
    pub note_id: String,
    pub activity_id: String,
}

impl MockPeer {
    /// Bind `127.0.0.1:0` and serve WebFinger + actor in the background.
    pub async fn bind(username: &str) -> std::io::Result<Self> {
        let key = RsaPrivateKey::new(&mut OsRng, 2048)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let pem = RsaPublicKey::from(&key)
            .to_public_key_pem(LineEnding::LF)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let actor_id = format!("http://{addr}/users/{username}");
        let notes = Arc::new(Mutex::new(Vec::new()));
        let state = PeerState {
            username: username.to_string(),
            actor_id: actor_id.clone(),
            pem,
            notes: notes.clone(),
        };
        let app = Router::new()
            .route("/.well-known/webfinger", get(webfinger))
            .route("/users/{name}", get(actor))
            .route("/users/{name}/outbox", get(outbox))
            .route("/users/{name}/avatar.png", get(avatar))
            .with_state(state);
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            addr,
            username: username.to_string(),
            actor_id,
            key,
            seq: AtomicU64::new(1),
            notes,
            server_task,
        })
    }

    /// Publish a public Note into this peer's outbox (no inbox POST).
    pub fn publish_note(&self, content: &str) -> String {
        let n = self.seq.fetch_add(1, Ordering::Relaxed);
        let note_id = format!("{}/statuses/{n}", self.actor_id);
        self.notes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((note_id.clone(), content.to_string()));
        note_id
    }

    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    /// `acct:user@127.0.0.1:port` — same DID as [`Self::actor_id`] via loopback http.
    pub fn acct(&self) -> String {
        format!("acct:{}@{}", self.username, self.addr)
    }

    pub async fn post_create_note(&self, inbox_url: &str, content: &str) -> Result<DeliveredNote> {
        let n = self.seq.fetch_add(1, Ordering::Relaxed);
        let note_id = format!("{}/statuses/{n}", self.actor_id);
        let activity_id = format!("{note_id}/activity");
        let note = post_to_note(&note_id, &self.actor_id, content, None, None);
        let activity = create_note(&activity_id, &self.actor_id, &note);
        let body = serde_json::to_vec(&activity)?;
        let key_id = format!("{}#main-key", self.actor_id);
        let (status, text) = post_signed(&self.key, &key_id, inbox_url, &body).await?;
        Ok(DeliveredNote {
            status,
            body: text,
            note_id,
            activity_id,
        })
    }
}

async fn post_signed(
    key: &RsaPrivateKey,
    key_id: &str,
    inbox_url: &str,
    body: &[u8],
) -> Result<(u16, String)> {
    let (path, host) = inbox_parts(inbox_url)?;
    let date = http_date(unix_now());
    let (signature, digest) = sign_request(key, key_id, "POST", &path, &host, &date, body);
    let resp = reqwest::Client::new()
        .post(inbox_url)
        .header("Host", host)
        .header("Date", date)
        .header("Digest", digest)
        .header("Signature", signature)
        .header("Content-Type", AP_JSON)
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| Error::Protocol(e.to_string()))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text))
}

fn inbox_parts(inbox_url: &str) -> Result<(String, String)> {
    let u = reqwest::Url::parse(inbox_url).map_err(|e| Error::Protocol(e.to_string()))?;
    let host = match (u.host_str(), u.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => return Err(Error::Protocol("inbox missing host".into())),
    };
    let path = match u.query() {
        Some(q) => format!("{}?{q}", u.path()),
        None => u.path().to_string(),
    };
    Ok((path, host))
}

#[derive(Deserialize)]
struct WfQuery {
    resource: Option<String>,
}

#[derive(Deserialize)]
struct OutboxQuery {
    page: Option<String>,
}

async fn outbox(
    State(st): State<PeerState>,
    Path(name): Path<String>,
    Query(q): Query<OutboxQuery>,
) -> Response {
    if !name.eq_ignore_ascii_case(&st.username) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let notes = st.notes.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let items: Vec<serde_json::Value> = notes
        .iter()
        .map(|(id, text)| {
            let note = post_to_note(id, &st.actor_id, text, None, None);
            create_note(&format!("{id}/activity"), &st.actor_id, &note)
        })
        .collect();
    let collection_id = format!("{}/outbox", st.actor_id);
    let first = format!("{collection_id}?page=true");
    let body = if q.page.as_deref().is_some_and(|p| p != "false") {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": first,
            "type": "OrderedCollectionPage",
            "partOf": collection_id,
            "orderedItems": items,
        })
    } else {
        serde_json::json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": collection_id,
            "type": "OrderedCollection",
            "totalItems": items.len(),
            "first": first,
        })
    };
    json_ct(StatusCode::OK, AP_JSON, &body)
}

async fn webfinger(State(st): State<PeerState>, Query(q): Query<WfQuery>) -> Response {
    let Some(resource) = q.resource.filter(|r| !r.is_empty()) else {
        return (StatusCode::BAD_REQUEST, "missing resource").into_response();
    };
    let rest = resource
        .strip_prefix("acct:")
        .or_else(|| resource.strip_prefix('@'))
        .unwrap_or(&resource);
    let user_ok = rest
        .split_once('@')
        .is_some_and(|(u, _)| u.eq_ignore_ascii_case(&st.username));
    if !user_ok && resource != st.actor_id {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let subject = format!("acct:{}@{}", st.username, host_of(&st.actor_id));
    json_ct(StatusCode::OK, JRD, &build_jrd(&subject, &st.actor_id))
}

async fn actor(State(st): State<PeerState>, Path(name): Path<String>) -> Response {
    if !name.eq_ignore_ascii_case(&st.username) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let inbox = format!("{}/inbox", st.actor_id);
    let outbox = format!("{}/outbox", st.actor_id);
    let mut doc = person_document(&st.actor_id, &st.username, &inbox, &outbox, &st.pem);
    doc["name"] = serde_json::json!("Demo");
    doc["summary"] = serde_json::json!("<p>Local mock Mastodon peer</p>");
    doc["icon"] = serde_json::json!({
        "type": "Image",
        "mediaType": "image/png",
        "url": format!("{}/avatar.png", st.actor_id)
    });
    json_ct(StatusCode::OK, AP_JSON, &doc)
}

async fn avatar(State(st): State<PeerState>, Path(name): Path<String>) -> Response {
    if !name.eq_ignore_ascii_case(&st.username) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .body(axum::body::Body::from(TINY_PNG.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn json_ct(status: StatusCode, ct: &str, v: &serde_json::Value) -> Response {
    let bytes = serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, ct)
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn host_of(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_webfinger_and_actor() {
        let peer = MockPeer::bind("demo").await.unwrap();
        let wf = format!(
            "http://{}/.well-known/webfinger?resource={}",
            peer.addr,
            peer.acct()
        );
        let jrd: serde_json::Value = reqwest::get(&wf).await.unwrap().json().await.unwrap();
        let parsed = crate::parse_jrd(&jrd).unwrap();
        assert_eq!(parsed.actor, peer.actor_id());
        let actor: serde_json::Value = reqwest::get(peer.actor_id())
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(actor["id"], peer.actor_id());
        assert_eq!(actor["preferredUsername"], "demo");
        assert!(actor["publicKey"]["publicKeyPem"]
            .as_str()
            .unwrap()
            .contains("BEGIN PUBLIC KEY"));
    }

    #[tokio::test]
    async fn serves_outbox_collection_and_first_page() {
        let peer = MockPeer::bind("alice").await.unwrap();
        let note_id = peer.publish_note("page item");
        let col: serde_json::Value = reqwest::get(format!("{}/outbox", peer.actor_id()))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(col["type"], "OrderedCollection");
        let first = col["first"].as_str().expect("first page url");
        let page: serde_json::Value = reqwest::get(first).await.unwrap().json().await.unwrap();
        let idx = crate::parse_outbox_page(&page);
        let notes: Vec<_> = idx
            .items
            .iter()
            .filter_map(crate::inbound_from_outbox_item)
            .collect();
        assert_eq!(notes.len(), 1);
        match &notes[0] {
            crate::Inbound::CreateNote { note, .. } => {
                assert_eq!(note["id"], note_id);
                assert_eq!(note["content"], "page item");
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn dropping_peer_releases_listener() {
        let peer = MockPeer::bind("lifecycle").await.unwrap();
        let address = peer.addr;
        assert!(tokio::net::TcpStream::connect(address).await.is_ok());
        drop(peer);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(listener) = tokio::net::TcpListener::bind(address).await {
                    drop(listener);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("dropping MockPeer must release its listening socket");
    }
}
