//! ActivityPub S2S facade: WebFinger, actor, inbox, outbox (same process).

mod ingest;
mod outbound;
mod ssrf;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rsa::pkcs8::{
    DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding,
};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Deserialize;
use tangled::http::{PutRecordRequest, PutRecordResponse};
use tangled::KnotEngine;
use tangled_ap::httpsig::{self, parse_signature};
use tangled_ap::{
    inbound_from_outbox_item, parse_inbound, parse_outbox_collection, parse_outbox_page,
    person_document, FirstPage, Inbound,
};

use crate::AppState;

pub use ingest::{ingest_activity, store_blob, upsert_profile};
pub use outbound::{deliveries_for, outbound_activity, RemoteActor};
pub use ssrf::{advertised_allows_loopback, delivery_url_allowed, inbox_host_matches_actor};

const AP_JSON: &str = "application/activity+json";
const JRD: &str = "application/jrd+json";

pub struct ApState {
    pub public_url: String,
    rsa: Arc<Mutex<Option<RsaPrivateKey>>>,
    pub remotes: Mutex<HashMap<String, RemoteActor>>,
    pub followers: Mutex<Vec<RemoteActor>>,
    pub deliveries: Mutex<Vec<serde_json::Value>>,
    actors: Mutex<HashMap<String, serde_json::Value>>,
    allow_loopback: AtomicBool,
}

impl ApState {
    pub fn new(public_url: String, rsa: Option<RsaPrivateKey>) -> Self {
        Self {
            public_url: public_url.trim_end_matches('/').to_string(),
            rsa: Arc::new(Mutex::new(rsa)),
            remotes: Mutex::new(HashMap::new()),
            followers: Mutex::new(Vec::new()),
            deliveries: Mutex::new(Vec::new()),
            actors: Mutex::new(HashMap::new()),
            allow_loopback: AtomicBool::new(false),
        }
    }

    pub fn set_allow_loopback(&self, allow: bool) {
        self.allow_loopback.store(allow, Ordering::Relaxed);
    }

    pub fn allow_loopback(&self) -> bool {
        self.allow_loopback.load(Ordering::Relaxed)
    }

    pub fn seed_actor(&self, doc: serde_json::Value) {
        if let Some(id) = doc.get("id").and_then(|i| i.as_str()) {
            self.actors
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id.to_string(), doc);
        }
    }

    pub fn actor_doc(&self, id: &str) -> Option<serde_json::Value> {
        self.actors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
    }

    pub fn remote_inbox(&self, actor: &str) -> Option<String> {
        self.remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(actor)
            .map(|r| r.inbox.clone())
    }

    pub fn rsa(&self) -> RsaPrivateKey {
        let mut g = self.rsa.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(k) = g.as_ref() {
            return k.clone();
        }
        let k = generate_rsa();
        *g = Some(k.clone());
        k
    }

    pub fn public_pem(&self) -> String {
        RsaPublicKey::from(&self.rsa())
            .to_public_key_pem(LineEnding::LF)
            .unwrap_or_default()
    }

    pub fn remember(&self, actor: RemoteActor) {
        let mut g = self.remotes.lock().unwrap_or_else(|e| e.into_inner());
        g.insert(actor.actor.clone(), actor);
    }
}

pub fn generate_rsa() -> RsaPrivateKey {
    let mut rng = rand::rngs::OsRng;
    RsaPrivateKey::new(&mut rng, 2048).expect("rsa")
}

pub fn load_or_create_rsa(dir: &std::path::Path) -> std::io::Result<RsaPrivateKey> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("ap-rsa.pem");
    if path.exists() {
        let pem = std::fs::read_to_string(&path)?;
        return RsaPrivateKey::from_pkcs8_pem(&pem)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()));
    }
    let key = generate_rsa();
    let pem = key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, pem.as_bytes())?;
    Ok(key)
}

pub fn ensure_activitypub(engine: &mut KnotEngine) {
    if let Some(d) = engine.describe.as_mut() {
        if !d.protocols.iter().any(|p| p == "activitypub") {
            d.protocols.push("activitypub".into());
        }
    }
}

#[derive(Deserialize)]
pub struct WfQuery {
    pub resource: Option<String>,
}

pub async fn webfinger(State(st): State<AppState>, Query(q): Query<WfQuery>) -> Response {
    let Some(resource) = q.resource.filter(|r| !r.is_empty()) else {
        return (StatusCode::BAD_REQUEST, "missing resource").into_response();
    };
    let host = host_of(&st.ap.public_url);
    let rest = resource
        .strip_prefix("acct:")
        .or_else(|| resource.strip_prefix('@'))
        .unwrap_or(&resource);
    let (name, res_host) = match rest.split_once('@') {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    if !res_host.eq_ignore_ascii_case(&host) && res_host != "localhost" {
        return (StatusCode::NOT_FOUND, "wrong host").into_response();
    }
    let actor = format!("{}/users/{name}", st.ap.public_url);
    let body = tangled_ap::build_jrd(&format!("acct:{name}@{host}"), &actor);
    json_ct(StatusCode::OK, JRD, &body)
}

pub async fn actor(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let id = format!("{}/users/{name}", st.ap.public_url);
    let inbox = format!("{id}/inbox");
    let outbox = format!("{id}/outbox");
    let doc = person_document(&id, &name, &inbox, &outbox, &st.ap.public_pem());
    json_ct(StatusCode::OK, AP_JSON, &doc)
}

pub async fn outbox(State(st): State<AppState>, Path(name): Path<String>) -> Response {
    let id = format!("{}/users/{name}/outbox", st.ap.public_url);
    let items = {
        let engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        outbound::outbox_notes(&engine, &st.ap.public_url, &name)
    };
    let body = serde_json::json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": id,
        "type": "OrderedCollection",
        "totalItems": items.len(),
        "orderedItems": items,
    });
    json_ct(StatusCode::OK, AP_JSON, &body)
}

/// WebFinger + actor fetch + public outbox pull (no HTTP signatures).
pub async fn resolve(State(st): State<AppState>, Query(q): Query<WfQuery>) -> Response {
    let Some(resource) = q.resource.filter(|r| !r.is_empty()) else {
        return (StatusCode::BAD_REQUEST, "missing resource").into_response();
    };
    match resolve_remote(&st, &resource).await {
        Ok(body) => json_ct(StatusCode::OK, "application/json", &body),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn resolve_remote(
    st: &AppState,
    resource: &str,
) -> std::result::Result<serde_json::Value, String> {
    let allow = st.ap.allow_loopback();
    let actor_url = if resource.starts_with("http://") || resource.starts_with("https://") {
        tangled::foreign::canonical_foreign_id("activitypub", resource)
            .map_err(|e| e.to_string())?
    } else {
        match webfinger_actor(resource, allow).await {
            Ok(url) => url,
            Err(_) => {
                tangled::foreign::activitypub_acct_actor(resource).map_err(|e| e.to_string())?
            }
        }
    };
    if let Ok(doc) = resolve_actor(&st.ap, &actor_url).await {
        remember_resolved(st, &actor_url, &doc);
        pull_outbox(st, &actor_url, &doc).await;
        upsert_remote_profile(st, &actor_url, &doc).await;
    }
    let id = tangled::foreign::activitypub_actor_identity(&actor_url);
    let handle = resource
        .strip_prefix("acct:")
        .or_else(|| resource.strip_prefix('@'))
        .unwrap_or(resource)
        .to_string();
    let display_name = st
        .ap
        .actor_doc(&actor_url)
        .as_ref()
        .and_then(tangled_ap::actor::display_name);
    let mut body = serde_json::json!({
        "did": id.did_string(),
        "actor": actor_url,
        "handle": handle,
    });
    if let Some(name) = display_name {
        body["displayName"] = serde_json::json!(name);
    }
    Ok(body)
}

async fn webfinger_actor(
    resource: &str,
    allow_loopback: bool,
) -> std::result::Result<String, String> {
    let heuristic =
        tangled::foreign::activitypub_acct_actor(resource).map_err(|e| e.to_string())?;
    let actor_url = reqwest::Url::parse(&heuristic).map_err(|e| e.to_string())?;
    let mut wf = actor_url.clone();
    wf.set_path("/.well-known/webfinger");
    wf.set_query(None);
    wf.query_pairs_mut().append_pair("resource", resource);
    let jrd = fetch_json(
        wf.as_str(),
        "application/jrd+json, application/json",
        allow_loopback,
    )
    .await?;
    let parsed = tangled_ap::parse_jrd(&jrd).map_err(|e| e.to_string())?;
    if !delivery_url_allowed(&parsed.actor, allow_loopback) {
        return Err("actor url not allowed".into());
    }
    if !inbox_host_matches_actor(&parsed.actor, &heuristic) {
        return Err("webfinger actor host mismatch".into());
    }
    Ok(parsed.actor)
}

fn remember_resolved(st: &AppState, actor_id: &str, doc: &serde_json::Value) {
    let allow = st.ap.allow_loopback();
    let Some(inbox) = tangled_ap::actor::actor_inbox(doc) else {
        return;
    };
    if !delivery_url_allowed(&inbox, allow) || !inbox_host_matches_actor(&inbox, actor_id) {
        return;
    }
    st.ap.remember(RemoteActor {
        actor: actor_id.to_string(),
        inbox,
        did: tangled::foreign::activitypub_actor_identity(actor_id).did_string(),
    });
}

async fn pull_outbox_for_did(st: &AppState, did: &str) {
    let remote = {
        let remotes = st.ap.remotes.lock().unwrap_or_else(|e| e.into_inner());
        remotes.values().find(|r| r.did == did).cloned()
    };
    let Some(remote) = remote else {
        return;
    };
    let Some(doc) = st.ap.actor_doc(&remote.actor) else {
        return;
    };
    pull_outbox(st, &remote.actor, &doc).await;
}

async fn pull_outbox(st: &AppState, actor_id: &str, actor: &serde_json::Value) {
    let items = match fetch_outbox_items(st, actor_id, actor).await {
        Ok(v) => v,
        Err(_) => {
            upsert_remote_profile(st, actor_id, actor).await;
            return;
        }
    };
    let translator = {
        let engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        engine
            .describe
            .as_ref()
            .map(|d| d.did.clone())
            .unwrap_or_default()
    };
    {
        let mut engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        for item in items {
            let Some(inbound) = inbound_from_outbox_item(&item) else {
                continue;
            };
            let Inbound::CreateNote { actor, .. } = &inbound else {
                continue;
            };
            if actor != actor_id {
                continue;
            }
            let raw = serde_json::to_vec(&item).unwrap_or_default();
            let _ = ingest::ingest_activity(
                &mut engine,
                &translator,
                &st.ap.public_url,
                &inbound,
                &raw,
            );
        }
    }
    upsert_remote_profile(st, actor_id, actor).await;
}

async fn fetch_outbox_items(
    st: &AppState,
    actor_id: &str,
    actor: &serde_json::Value,
) -> std::result::Result<Vec<serde_json::Value>, String> {
    let allow = st.ap.allow_loopback();
    let outbox_url =
        tangled_ap::actor::actor_outbox(actor).ok_or_else(|| "actor missing outbox".to_string())?;
    if !delivery_url_allowed(&outbox_url, allow) || !inbox_host_matches_actor(&outbox_url, actor_id)
    {
        return Err("outbox url not allowed".into());
    }
    let doc = fetch_json(&outbox_url, AP_ACCEPT, allow).await?;
    let idx = parse_outbox_collection(&doc);
    let mut items = idx.items;
    match idx.first {
        FirstPage::Url(url) => {
            if delivery_url_allowed(&url, allow) && inbox_host_matches_actor(&url, actor_id) {
                if let Ok(page) = fetch_json(&url, AP_ACCEPT, allow).await {
                    items.extend(parse_outbox_page(&page).items);
                }
            }
        }
        FirstPage::Embedded(page) => items.extend(parse_outbox_page(&page).items),
        FirstPage::None => {}
    }
    Ok(items)
}

const AP_ACCEPT: &str = "application/activity+json, application/ld+json";
const AVATAR_MAX: usize = 2 * 1024 * 1024;

async fn upsert_remote_profile(st: &AppState, actor_id: &str, doc: &serde_json::Value) {
    let allow = st.ap.allow_loopback();
    let mapped = tangled_ap::actor_to_profile(doc, actor_id);
    let avatar_cid = match mapped.icon_url.as_deref() {
        Some(url) => fetch_icon_cid(st, actor_id, url, allow).await,
        None => None,
    };
    let mut engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
    upsert_profile(&mut engine, actor_id, Some(doc), avatar_cid.as_deref());
}

async fn fetch_icon_cid(
    st: &AppState,
    actor_id: &str,
    url: &str,
    allow_loopback: bool,
) -> Option<String> {
    let bytes = fetch_bytes(url, allow_loopback, AVATAR_MAX).await.ok()?;
    if !is_image(&bytes) {
        return None;
    }
    let did = tangled::foreign::activitypub_actor_identity(actor_id).did_string();
    let mut engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
    Some(store_blob(&mut engine, &did, &bytes))
}

async fn fetch_bytes(
    url: &str,
    allow_loopback: bool,
    max: usize,
) -> std::result::Result<Vec<u8>, String> {
    if !delivery_url_allowed(url, allow_loopback) {
        return Err("url not allowed".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("tangled-knot")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("fetch {url} {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("avatar too large".into());
    }
    Ok(bytes.to_vec())
}

fn is_image(bytes: &[u8]) -> bool {
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return true;
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return true;
    }
    if bytes.starts_with(b"GIF8") {
        return true;
    }
    bytes.len() > 12 && &bytes[8..12] == b"WEBP"
}

async fn fetch_json(
    url: &str,
    accept: &str,
    allow_loopback: bool,
) -> std::result::Result<serde_json::Value, String> {
    if !delivery_url_allowed(url, allow_loopback) {
        return Err("url not allowed".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("tangled-knot")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .header("Accept", accept)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("fetch {url} {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn inbox(
    State(st): State<AppState>,
    headers: HeaderMap,
    req: axum::http::Request<Body>,
) -> Response {
    let path = req.uri().path().to_string();
    let host = header_str(&headers, "host").unwrap_or_else(|| host_of(&st.ap.public_url));
    let body = match axum::body::to_bytes(req.into_body(), crate::BODY_LIMIT).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let hdrs: Vec<(String, String)> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|val| (k.as_str().to_string(), val.to_string()))
        })
        .collect();
    let activity: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("json: {e}")).into_response();
        }
    };
    if let Err(e) = verify_inbox(&st.ap, &activity, &hdrs, &path, &host, &body).await {
        return (StatusCode::UNAUTHORIZED, e).into_response();
    }
    let inbound = match parse_inbound(&activity) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    remember_actor(&st, &activity);
    let actor_doc = actor_id_of(&activity).and_then(|id| st.ap.actor_doc(&id));
    let translator = {
        let engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        engine
            .describe
            .as_ref()
            .map(|d| d.did.clone())
            .unwrap_or_default()
    };
    let ingested = {
        let mut engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        match ingest::ingest_activity(&mut engine, &translator, &st.ap.public_url, &inbound, &body)
        {
            Ok(v) => v,
            Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        }
    };
    if let tangled_ap::Inbound::Follow { actor, .. } = &inbound {
        if let Some(remote) = st
            .ap
            .remotes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(actor)
            .cloned()
        {
            st.ap
                .followers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(remote);
        }
    }
    if let Some(doc) = actor_doc {
        if let Some(actor_id) = actor_id_of(&activity) {
            upsert_remote_profile(&st, &actor_id, &doc).await;
        }
    }
    let _ = ingested;
    StatusCode::ACCEPTED.into_response()
}

pub fn after_put_record(st: &AppState, record: &serde_json::Value) {
    if record.get("collection").and_then(|c| c.as_str()) == Some("tangled.graph.follow") {
        if let Some(subject) = record
            .get("body")
            .and_then(|b| b.get("subject"))
            .and_then(|s| s.as_str())
        {
            let st = st.clone();
            let did = subject.to_string();
            tokio::spawn(async move {
                pull_outbox_for_did(&st, &did).await;
            });
        }
    }
    let Some(activity) = outbound_activity(&st.ap, record) else {
        return;
    };
    st.ap
        .deliveries
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(activity.clone());
    let targets = deliveries_for(&st.ap, record);
    if targets.is_empty() {
        return;
    }
    let rsa = st.ap.rsa();
    let public_url = st.ap.public_url.clone();
    let allow_loopback = st.ap.allow_loopback();
    tokio::spawn(async move {
        outbound::deliver_signed(&rsa, &public_url, &activity, &targets, allow_loopback).await;
    });
}

async fn verify_inbox(
    ap: &ApState,
    activity: &serde_json::Value,
    headers: &[(String, String)],
    path: &str,
    host: &str,
    body: &[u8],
) -> std::result::Result<(), String> {
    let actor_id = actor_id_of(activity).ok_or_else(|| "missing actor".to_string())?;
    let parsed = parse_signature(header_lookup(headers, "signature").unwrap_or(""))
        .map_err(|e| e.to_string())?;
    if parsed.key_id.is_empty() {
        return Err("missing keyId".into());
    }
    if !key_id_binds_actor(&parsed.key_id, &actor_id) {
        return Err("keyId does not bind to actor".into());
    }
    let doc = resolve_actor(ap, &actor_id).await?;
    let owner = tangled_ap::actor::public_key_owner(&doc).ok_or("missing publicKey owner")?;
    if owner != actor_id {
        return Err("publicKey owner mismatch".into());
    }
    if let Some(kid) = tangled_ap::actor::public_key_id(&doc) {
        if kid != parsed.key_id {
            return Err("keyId does not match actor publicKey.id".into());
        }
    }
    let pem = tangled_ap::actor::public_key_pem(&doc).ok_or("missing publicKeyPem")?;
    let pubk = RsaPublicKey::from_public_key_pem(&pem).map_err(|e| e.to_string())?;
    httpsig::verify_request(&pubk, "POST", path, headers, body).map_err(|e| e.to_string())?;
    let _ = host;
    Ok(())
}

fn actor_id_of(activity: &serde_json::Value) -> Option<String> {
    let a = activity.get("actor")?;
    if let Some(s) = a.as_str() {
        return Some(s.to_string());
    }
    a.get("id")?.as_str().map(str::to_string)
}

fn key_id_binds_actor(key_id: &str, actor_id: &str) -> bool {
    let actor_id = actor_id.trim_end_matches('/');
    key_id == actor_id
        || key_id.starts_with(&format!("{actor_id}#"))
        || key_id.starts_with(&format!("{actor_id}/"))
}

async fn resolve_actor(
    ap: &ApState,
    actor_id: &str,
) -> std::result::Result<serde_json::Value, String> {
    if let Some(doc) = ap.actor_doc(actor_id) {
        return Ok(doc);
    }
    if !delivery_url_allowed(actor_id, ap.allow_loopback()) {
        return Err("actor url not allowed".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(actor_id)
        .header("Accept", "application/activity+json, application/ld+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("actor fetch {}", resp.status()));
    }
    let doc: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let id = doc.get("id").and_then(|i| i.as_str()).unwrap_or("");
    if id != actor_id {
        return Err("fetched actor id mismatch".into());
    }
    ap.seed_actor(doc.clone());
    Ok(doc)
}

fn remember_actor(st: &AppState, activity: &serde_json::Value) {
    let Some(actor_id) = actor_id_of(activity) else {
        return;
    };
    let allow = st.ap.allow_loopback();
    let inbox = st
        .ap
        .actor_doc(&actor_id)
        .as_ref()
        .and_then(tangled_ap::actor::actor_inbox)
        .or_else(|| {
            activity.get("actor").and_then(|a| {
                if a.is_object() {
                    tangled_ap::actor::actor_inbox(a)
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| format!("{actor_id}/inbox"));
    if !delivery_url_allowed(&inbox, allow) {
        return;
    }
    if !inbox_host_matches_actor(&inbox, &actor_id) {
        return;
    }
    st.ap.remember(RemoteActor {
        actor: actor_id.clone(),
        inbox,
        did: tangled::foreign::activitypub_actor_identity(&actor_id).did_string(),
    });
}

fn json_ct(status: StatusCode, ct: &str, v: &serde_json::Value) -> Response {
    let bytes = serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, ct)
        .body(Body::from(bytes))
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

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn header_lookup<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Inspect a successful `putRecord` body for outbound AP.
pub fn put_record_json(req: &tangled::http::KnotRequest) -> Option<serde_json::Value> {
    if req.path.trim_end_matches('/') != "/xrpc/tangled.repo.putRecord" {
        return None;
    }
    let body = req.body.as_deref()?;
    let parsed: PutRecordRequest = serde_json::from_slice(body).ok()?;
    Some(parsed.record)
}

pub fn put_ok(resp: &tangled::http::KnotResponse) -> bool {
    if resp.status != 200 {
        return false;
    }
    serde_json::from_slice::<PutRecordResponse>(&resp.body)
        .ok()
        .is_some_and(|p| p.cid.is_some())
}
