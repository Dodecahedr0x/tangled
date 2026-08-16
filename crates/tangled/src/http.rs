//! Typed knot HTTP client (spec 12 / OpenAPI). Transport is pluggable so tests
//! run against an in-process mock without the network.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const ACCEPT: &str = "application/cbor, application/json";

#[derive(Clone, Copy, Debug)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug)]
pub struct KnotRequest {
    pub method: Method,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub content_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KnotResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    /// `Set-Cookie` from SIWS verify (engine / HTTP).
    pub set_cookie: Option<String>,
    /// Extra response headers (`Retry-After`, `Content-Length`, `X-Tangled-Media-Type`).
    pub headers: Vec<(String, String)>,
}

impl KnotResponse {
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        if self.content_type.contains("cbor") {
            let v = crate::dagcbor::decode_json(&self.body)
                .map_err(|e| Error::Transport(format!("cbor: {e}")))?;
            return serde_json::from_value(v)
                .map_err(|e| Error::Transport(format!("cbor json: {e}")));
        }
        serde_json::from_slice(&self.body).map_err(|e| Error::Transport(format!("json: {e}")))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn error_if_not_ok(&self) -> Result<()> {
        if (200..300).contains(&self.status) || self.status == 204 {
            return Ok(());
        }
        if let Ok(err) = self.json::<ApiError>() {
            return Err(Error::Http {
                status: self.status,
                error: err.error,
            });
        }
        Err(Error::Http {
            status: self.status,
            error: String::from_utf8_lossy(&self.body).into_owned(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    #[serde(default)]
    pub retry: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeDescribe {
    pub v: u32,
    pub did: String,
    pub profiles: Vec<String>,
    pub protocols: Vec<String>,
    #[serde(rename = "maxBlob")]
    pub max_blob: u64,
    #[serde(rename = "custodialSign", default)]
    pub custodial_sign: bool,
    /// Operator-marked public knot base URLs. Empty when none. Never learned URLs.
    #[serde(default)]
    pub peers: Vec<String>,
}

/// `GET /xrpc/tangled.node.stats` — knot-local public aggregates (spec 12).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeStats {
    pub v: u32,
    pub did: String,
    #[serde(rename = "asOf")]
    pub as_of: u64,
    pub counts: NodeCounts,
    pub rates: NodeRates,
    pub topology: NodeTopology,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeCounts {
    pub identities: u64,
    pub repos: u64,
    pub records: u64,
    pub blobs: u64,
    pub peers: u64,
    pub circles: u64,
    pub foreign: u64,
    pub commits: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeRates {
    #[serde(rename = "publicCommits")]
    pub public_commits: u64,
    pub retained: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeTopology {
    pub peers: Vec<String>,
    pub hosted: Vec<HostedIdentity>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostedIdentity {
    pub did: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    pub records: u64,
}

/// Absolute `http://` / `https://` knot base. Trailing slash stripped. Rejects userinfo.
pub fn normalize_peer_url(raw: &str) -> Option<String> {
    let s = raw.trim().trim_end_matches('/');
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))?;
    if rest.is_empty() || rest.contains([' ', '\t', '\n', '\r', '@']) {
        return None;
    }
    Some(s.to_string())
}

pub fn merge_peer_urls(into: &mut Vec<String>, urls: impl IntoIterator<Item = impl AsRef<str>>) {
    for raw in urls {
        if let Some(n) = normalize_peer_url(raw.as_ref()) {
            if !into.contains(&n) {
                into.push(n);
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetHeadResponse {
    pub did: String,
    /// Signed Commit CID (not the raw MST root).
    pub commit: String,
    pub rev: String,
    pub knot: String,
    /// MST root CID (`Commit.data`). Empty on legacy heads.
    #[serde(default)]
    pub data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GetRecordResponse {
    pub record: serde_json::Value,
    pub proof: Vec<serde_json::Value>,
    pub commit: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetBlocksRequest {
    pub cids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetBlocksResponse {
    #[serde(default)]
    pub missing: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetRepoAuthRequest {
    pub did: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blobs: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyRequest {
    pub did: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutRecordRequest {
    pub record: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutRecordResponse {
    #[serde(default)]
    pub cid: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutBlobResponse {
    pub cid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportResponse {
    pub did: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct IdentityResolveResponse {
    pub did: String,
    #[serde(rename = "idCid", default)]
    pub id_cid: Option<String>,
    #[serde(default)]
    pub endpoints: Vec<String>,
    /// Generative or operational `did:sol` document (spec 04 / 13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<serde_json::Value>,
}

/// `GET /xrpc/tangled.ap.resolve?resource=` — WebFinger / actor URL through the knot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApResolveResponse {
    pub did: String,
    pub actor: String,
    pub handle: String,
    #[serde(
        rename = "displayName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapVerifyResponse {
    pub ok: bool,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub res: Option<String>,
}

/// In-process stand-in for a getRepo CAR of one collection (spec 11).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ListRecordsResponse {
    pub records: Vec<ListRecordItem>,
    #[serde(default)]
    pub commit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ListRecordItem {
    pub collection: String,
    pub rkey: String,
    pub cid: String,
    pub record: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacklinkHit {
    pub did: String,
    pub collection: String,
    pub rkey: String,
    pub cid: String,
    pub commit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacklinkQuery {
    pub hits: Vec<BacklinkHit>,
    #[serde(default)]
    pub count: Option<u64>,
    pub overlay: bool,
    #[serde(default)]
    pub complete: bool,
}

/// Overlay comment count from `queryBacklinks` / `loom.query` (spec 11 / 14).
///
/// An empty overlay is this observer's count, not proof the network has zero comments.
/// `MethodNotImplemented` means the knot has no index — also not "zero comments".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverlayCount {
    /// Knot returned `MethodNotImplemented`.
    IndexMissing,
    /// Local overlay. `complete` MUST be false in v1.
    Observed { count: u64, complete: bool },
}

pub fn overlay_comment_count(result: Result<BacklinkQuery>) -> OverlayCount {
    match result {
        Err(Error::Http { error, .. }) if error == "MethodNotImplemented" => {
            OverlayCount::IndexMissing
        }
        Ok(q) => OverlayCount::Observed {
            count: q.count.unwrap_or(q.hits.len() as u64),
            complete: q.complete,
        },
        Err(_) => OverlayCount::IndexMissing,
    }
}

/// Empty overlay MUST NOT be treated as a network-wide zero.
pub fn overlay_proves_zero_comments(obs: &OverlayCount) -> bool {
    let _ = obs;
    false
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeMessage {
    pub t: String,
    #[serde(default)]
    pub did: String,
    pub commit: String,
    #[serde(default)]
    pub ops: Vec<SubscribeOp>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeOp {
    pub action: String,
    pub collection: String,
    pub rkey: String,
    pub cid: String,
}

pub use crate::knot::{KnotEngine, MockKnot};

pub(crate) fn json_ok<T: Serialize>(v: &T) -> Result<KnotResponse> {
    Ok(KnotResponse {
        status: 200,
        content_type: "application/json".into(),
        body: serde_json::to_vec(v).map_err(|e| Error::Transport(e.to_string()))?,
        set_cookie: None,
        headers: vec![],
    })
}

pub(crate) fn query(q: &[(String, String)], name: &str) -> Result<String> {
    q.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| Error::Http {
            status: 400,
            error: "InvalidRequest".into(),
        })
}

enum Backend {
    Mock(Arc<Mutex<MockKnot>>),
    Http {
        client: reqwest::Client,
        base: String,
    },
}

/// Knot HTTP client. Construct with [`KnotClient::mock`] or [`KnotClient::http`].
pub struct KnotClient {
    backend: Backend,
    authorization: Option<String>,
    session_cookie: Option<String>,
    /// Public peer URLs remembered from successful `describe` responses.
    learned_peers: Arc<Mutex<Vec<String>>>,
}

impl KnotClient {
    pub fn mock(knot: MockKnot) -> Self {
        Self {
            backend: Backend::Mock(Arc::new(Mutex::new(knot))),
            authorization: None,
            session_cookie: None,
            learned_peers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn mock_shared(knot: Arc<Mutex<MockKnot>>) -> Self {
        Self {
            backend: Backend::Mock(knot),
            authorization: None,
            session_cookie: None,
            learned_peers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn http(base_url: impl Into<String>) -> Self {
        Self {
            backend: Backend::Http {
                client: reqwest::Client::new(),
                base: base_url.into().trim_end_matches('/').to_string(),
            },
            authorization: None,
            session_cookie: None,
            learned_peers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Public peer URLs this client has seen on successful `describe` calls.
    pub fn learned_peers(&self) -> Vec<String> {
        self.learned_peers
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn remember_peers(&self, peers: &[String]) {
        if let Ok(mut g) = self.learned_peers.lock() {
            merge_peer_urls(&mut g, peers);
        }
    }

    pub fn with_cap_header(mut self, header: impl Into<String>) -> Self {
        self.authorization = Some(header.into());
        self
    }

    pub fn with_session_cookie(mut self, token: impl Into<String>) -> Self {
        self.session_cookie = Some(token.into());
        self
    }

    async fn send(&self, mut req: KnotRequest) -> Result<KnotResponse> {
        if let Some(h) = &self.authorization {
            req.headers.push(("Authorization".into(), h.clone()));
        }
        if let Some(t) = &self.session_cookie {
            req.headers.push((
                "Cookie".into(),
                format!("{}={t}", crate::session::COOKIE_NAME),
            ));
        }
        req.headers.push(("Accept".into(), ACCEPT.into()));
        match &self.backend {
            Backend::Mock(k) => {
                let mut guard = k
                    .lock()
                    .map_err(|_| Error::Transport("mock knot poisoned".into()))?;
                guard.handle(&req)
            }
            Backend::Http { client, base } => http_send(client, base, req).await,
        }
    }

    pub async fn describe(&self) -> Result<NodeDescribe> {
        let resp = self
            .send(get("/xrpc/tangled.node.describe", vec![]))
            .await?;
        resp.error_if_not_ok()?;
        let d: NodeDescribe = resp.json()?;
        self.remember_peers(&d.peers);
        Ok(d)
    }

    pub async fn stats(&self) -> Result<NodeStats> {
        let resp = self.send(get("/xrpc/tangled.node.stats", vec![])).await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn get_head(&self, did: &str) -> Result<GetHeadResponse> {
        let resp = self
            .send(get("/xrpc/tangled.sync.getHead", vec![("did", did)]))
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn get_record(
        &self,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<GetRecordResponse> {
        let resp = self
            .send(get(
                "/xrpc/tangled.sync.getRecord",
                vec![("did", did), ("collection", collection), ("rkey", rkey)],
            ))
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    /// List stored records for a DID (in-process stand-in for a getRepo CAR).
    pub async fn list_records(
        &self,
        did: &str,
        collection: Option<&str>,
    ) -> Result<ListRecordsResponse> {
        let mut q = vec![("did", did.to_string())];
        if let Some(c) = collection {
            q.push(("collection", c.to_string()));
        }
        let qref: Vec<(&str, &str)> = q.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp = self
            .send(get("/xrpc/tangled.sync.listRecords", qref))
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn get_blob(&self, did: &str, cid: &str) -> Result<Vec<u8>> {
        Ok(self.get_blob_response(did, cid).await?.body)
    }

    pub async fn get_blob_response(&self, did: &str, cid: &str) -> Result<KnotResponse> {
        let resp = self
            .send(get(
                "/xrpc/tangled.sync.getBlob",
                vec![("did", did), ("cid", cid)],
            ))
            .await?;
        resp.error_if_not_ok()?;
        Ok(resp)
    }

    pub async fn get_blocks(&self, body: &GetBlocksRequest) -> Result<KnotResponse> {
        let resp = self
            .send(post_json("/xrpc/tangled.sync.getBlocks", body)?)
            .await?;
        resp.error_if_not_ok()?;
        Ok(resp)
    }

    pub async fn get_repo(
        &self,
        did: &str,
        since: Option<&str>,
        blobs: Option<bool>,
    ) -> Result<Vec<u8>> {
        let mut q = vec![("did", did.to_string())];
        if let Some(s) = since {
            q.push(("since", s.to_string()));
        }
        if let Some(b) = blobs {
            q.push(("blobs", b.to_string()));
        }
        let qref: Vec<(&str, &str)> = q.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp = self.send(get("/xrpc/tangled.sync.getRepo", qref)).await?;
        resp.error_if_not_ok()?;
        Ok(resp.body)
    }

    pub async fn get_repo_auth(&self, body: &GetRepoAuthRequest) -> Result<Vec<u8>> {
        let resp = self
            .send(post_json("/xrpc/tangled.sync.getRepoAuth", body)?)
            .await?;
        resp.error_if_not_ok()?;
        Ok(resp.body)
    }

    pub async fn query_backlinks(
        &self,
        cid: &str,
        collection: Option<&str>,
        limit: Option<u32>,
    ) -> Result<BacklinkQuery> {
        let mut q = vec![("cid", cid.to_string())];
        if let Some(c) = collection {
            q.push(("collection", c.to_string()));
        }
        if let Some(l) = limit {
            q.push(("limit", l.to_string()));
        }
        let qref: Vec<(&str, &str)> = q.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp = self
            .send(get("/xrpc/tangled.sync.queryBacklinks", qref))
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn notify(&self, body: &NotifyRequest) -> Result<()> {
        let resp = self
            .send(post_json("/xrpc/tangled.sync.notify", body)?)
            .await?;
        resp.error_if_not_ok()
    }

    pub async fn put_record(&self, body: &PutRecordRequest) -> Result<PutRecordResponse> {
        let resp = self
            .send(post_json("/xrpc/tangled.repo.putRecord", body)?)
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn put_blob(&self, bytes: Vec<u8>) -> Result<PutBlobResponse> {
        self.put_blob_media(bytes, None).await
    }

    pub async fn put_blob_media(
        &self,
        bytes: Vec<u8>,
        media_type: Option<&str>,
    ) -> Result<PutBlobResponse> {
        let query = media_type
            .map(|t| vec![("mediaType".into(), t.to_string())])
            .unwrap_or_default();
        let resp = self
            .send(KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.putBlob".into(),
                query,
                headers: vec![],
                body: Some(bytes),
                content_type: Some("application/octet-stream".into()),
            })
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn import_repo(&self, car: Vec<u8>, force: bool) -> Result<ImportResponse> {
        let q = if force {
            vec![("force".into(), "true".into())]
        } else {
            vec![]
        };
        let resp = self
            .send(KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.repo.import".into(),
                query: q,
                headers: vec![],
                body: Some(car),
                content_type: Some("application/vnd.ipld.car".into()),
            })
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn ap_resolve(&self, resource: &str) -> Result<ApResolveResponse> {
        let resp = self
            .send(get(
                "/xrpc/tangled.ap.resolve",
                vec![("resource", resource)],
            ))
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn identity_resolve(
        &self,
        handle: Option<&str>,
        did: Option<&str>,
    ) -> Result<IdentityResolveResponse> {
        let mut q = Vec::new();
        if let Some(h) = handle {
            q.push(("handle", h));
        }
        if let Some(d) = did {
            q.push(("did", d));
        }
        let resp = self.send(get("/xrpc/tangled.identity.resolve", q)).await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn cap_verify(&self, invocation_cbor: Vec<u8>) -> Result<CapVerifyResponse> {
        let resp = self
            .send(KnotRequest {
                method: Method::Post,
                path: "/xrpc/tangled.cap.verify".into(),
                query: vec![],
                headers: vec![],
                body: Some(invocation_cbor),
                content_type: Some("application/cbor".into()),
            })
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn loom_query(&self, q: &str, ty: Option<&str>) -> Result<BacklinkQuery> {
        let mut query = vec![("q", q.to_string())];
        if let Some(t) = ty {
            query.push(("type", t.to_string()));
        }
        let qref: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let resp = self.send(get("/xrpc/tangled.loom.query", qref)).await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn subscribe(&self, did: &str, collection: Option<&str>) -> Result<KnotResponse> {
        let mut q = vec![("did", did.to_string())];
        if let Some(c) = collection {
            q.push(("collection", c.to_string()));
        }
        let qref: Vec<(&str, &str)> = q.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.send(get("/xrpc/tangled.sync.subscribe", qref)).await
    }

    pub async fn loom_firehose(&self) -> Result<KnotResponse> {
        self.send(get("/xrpc/tangled.loom.firehose", vec![])).await
    }

    pub async fn session_challenge(&self, did: &str) -> Result<crate::session::SiwsChallenge> {
        let resp = self
            .send(get("/xrpc/tangled.session.challenge", vec![("did", did)]))
            .await?;
        resp.error_if_not_ok()?;
        resp.json()
    }

    pub async fn session_verify(
        &self,
        body: &crate::session::SiwsVerifyRequest,
    ) -> Result<(crate::session::SiwsVerifyResponse, String)> {
        let resp = self
            .send(post_json("/xrpc/tangled.session.verify", body)?)
            .await?;
        resp.error_if_not_ok()?;
        let token = resp
            .set_cookie
            .as_deref()
            .and_then(cookie_token_from_set_cookie)
            .ok_or_else(|| Error::Transport("SIWS verify missing Set-Cookie".into()))?;
        Ok((resp.json()?, token))
    }
}

fn cookie_token_from_set_cookie(header: &str) -> Option<String> {
    let first = header.split(';').next()?.trim();
    first
        .strip_prefix(&format!("{}=", crate::session::COOKIE_NAME))
        .map(str::to_string)
}

fn get(path: &str, query: Vec<(&str, &str)>) -> KnotRequest {
    KnotRequest {
        method: Method::Get,
        path: path.into(),
        query: query
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
        headers: vec![],
        body: None,
        content_type: None,
    }
}

fn post_json<T: Serialize>(path: &str, body: &T) -> Result<KnotRequest> {
    Ok(KnotRequest {
        method: Method::Post,
        path: path.into(),
        query: vec![],
        headers: vec![],
        body: Some(serde_json::to_vec(body).map_err(|e| Error::Transport(e.to_string()))?),
        content_type: Some("application/json".into()),
    })
}

async fn http_send(client: &reqwest::Client, base: &str, req: KnotRequest) -> Result<KnotResponse> {
    let mut url = format!("{base}{}", req.path);
    if !req.query.is_empty() {
        let qs: String = req
            .query
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding_lite(k), urlencoding_lite(v)))
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&qs);
    }
    let mut builder = match req.method {
        Method::Get => client.get(&url),
        Method::Post => client.post(&url),
    };
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    if let Some(ct) = &req.content_type {
        builder = builder.header("content-type", ct);
    }
    if let Some(body) = req.body {
        builder = builder.body(body);
    }
    let resp = builder
        .send()
        .await
        .map_err(|e| Error::Transport(e.to_string()))?;
    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let headers = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|val| (k.as_str().to_string(), val.to_string()))
        })
        .collect();
    let body = resp
        .bytes()
        .await
        .map_err(|e| Error::Transport(e.to_string()))?
        .to_vec();
    Ok(KnotResponse {
        status,
        content_type,
        body,
        set_cookie,
        headers,
    })
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
