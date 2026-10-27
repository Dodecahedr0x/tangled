//! Tangled knot HTTP server. Admission lives in [`tangled::KnotEngine`]; this
//! crate is the axum binding, filesystem identity, and WebSocket subscribe.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tangled::http::{
    merge_peer_urls, normalize_peer_url, ApiError, KnotClient, KnotRequest, KnotResponse,
    Method as KnotMethod, SubscribeMessage,
};
use tangled::identity::Identity;
use tangled::{Error, KnotEngine, Result as TangledResult};
use tokio::sync::broadcast;

pub mod imports;
mod ws;

#[cfg(feature = "activitypub")]
pub mod ap;

pub(crate) const BODY_LIMIT: usize = 136_314_880;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Mutex<KnotEngine>>,
    pub bus: broadcast::Sender<SubscribeMessage>,
    pub live_clock: bool,
    #[cfg(feature = "activitypub")]
    pub ap: Arc<ap::ApState>,
}

impl AppState {
    pub fn new(mut engine: KnotEngine, live_clock: bool) -> Self {
        let (bus, _) = broadcast::channel(64);
        #[cfg(feature = "activitypub")]
        {
            ap::ensure_activitypub(&mut engine);
        }
        #[cfg(feature = "activitypub")]
        let public_url = engine
            .knot_base
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:0".into());
        #[cfg(feature = "activitypub")]
        let rsa = engine
            .data_dir
            .as_ref()
            .and_then(|d| ap::load_or_create_rsa(d).ok());
        #[cfg(feature = "activitypub")]
        let ap = {
            let ap = ap::ApState::new(public_url.clone(), rsa);
            if ap::advertised_allows_loopback(&public_url) {
                ap.set_allow_loopback(true);
            }
            Arc::new(ap)
        };
        Self {
            engine: Arc::new(Mutex::new(engine)),
            bus,
            live_clock,
            #[cfg(feature = "activitypub")]
            ap,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let r = Router::new()
        .route("/xrpc/tangled.sync.subscribe", get(ws::subscribe))
        .route("/xrpc/tangled.loom.firehose", get(ws::firehose));
    #[cfg(feature = "activitypub")]
    let r = r
        .route("/.well-known/webfinger", axum::routing::get(ap::webfinger))
        .route("/users/{name}", axum::routing::get(ap::actor))
        .route("/users/{name}/inbox", axum::routing::post(ap::inbox))
        .route("/users/{name}/outbox", axum::routing::get(ap::outbox))
        .route("/inbox", axum::routing::post(ap::inbox))
        .route("/xrpc/tangled.ap.resolve", axum::routing::get(ap::resolve));
    let app = r.fallback(xrpc).with_state(state.clone());
    if let Some(imports) = imports::self_hosted(&state) {
        app.merge(imports)
    } else {
        app
    }
}

pub async fn serve_listener(
    engine: KnotEngine,
    listener: tokio::net::TcpListener,
    live_clock: bool,
) -> std::io::Result<()> {
    axum::serve(listener, router(AppState::new(engine, live_clock))).await
}

/// Bind `127.0.0.1:0` and serve in the background. Used by integration tests.
pub async fn bind_ephemeral(
    mut engine: KnotEngine,
    live_clock: bool,
) -> std::io::Result<(SocketAddr, AppState)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    engine = engine.with_knot_base(format!("http://{addr}"));
    let state = AppState::new(engine, live_clock);
    #[cfg(feature = "activitypub")]
    {
        state.ap.set_allow_loopback(true);
    }
    let serve_state = state.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(serve_state)).await;
    });
    Ok((addr, state))
}

/// CLI flags for `tangled-knot`. `--peer` may be repeated.
#[derive(Debug, Clone)]
pub struct KnotCli {
    pub listen: String,
    pub data: PathBuf,
    pub public_url: Option<String>,
    pub peers: Vec<String>,
    pub help: bool,
}

impl KnotCli {
    pub fn parse<I, S>(args: I) -> std::result::Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut listen = "127.0.0.1:3000".to_string();
        let mut data = PathBuf::from("./data");
        let mut public_url = None;
        let mut peers = Vec::new();
        let mut help = false;
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_ref() {
                "--listen" => {
                    listen = it
                        .next()
                        .ok_or_else(|| "--listen needs ADDR".to_string())?
                        .as_ref()
                        .to_string();
                }
                "--data" => {
                    data = PathBuf::from(
                        it.next()
                            .ok_or_else(|| "--data needs DIR".to_string())?
                            .as_ref(),
                    );
                }
                "--public-url" => {
                    public_url = Some(
                        it.next()
                            .ok_or_else(|| "--public-url needs URL".to_string())?
                            .as_ref()
                            .to_string(),
                    );
                }
                "--peer" => {
                    let raw = it.next().ok_or_else(|| "--peer needs URL".to_string())?;
                    let n = normalize_peer_url(raw.as_ref()).ok_or_else(|| {
                        format!("--peer needs an absolute http(s) URL, got {}", raw.as_ref())
                    })?;
                    if !peers.contains(&n) {
                        peers.push(n);
                    }
                }
                "-h" | "--help" => help = true,
                other => return Err(format!("unknown argument {other}")),
            }
        }
        Ok(Self {
            listen,
            data,
            public_url,
            peers,
            help,
        })
    }
}

/// Operator-marked public peers: CLI `--peer`, `TANGLED_PEERS` (comma/space), `{data}/peers.txt`.
pub fn collect_public_peers(cli: &[String], data_dir: &Path, env: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    merge_peer_urls(&mut out, cli);
    if let Some(env) = env {
        merge_peer_urls(
            &mut out,
            env.split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty()),
        );
    }
    let file = data_dir.join("peers.txt");
    if let Ok(text) = std::fs::read_to_string(file) {
        merge_peer_urls(
            &mut out,
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#')),
        );
    }
    out
}

/// `describe` a remote knot and store its `peers` locally. Does not re-advertise them.
pub async fn learn_from_remote(
    engine: &Mutex<KnotEngine>,
    base: &str,
) -> TangledResult<Vec<String>> {
    let d = KnotClient::http(base).describe().await?;
    let mut g = engine
        .lock()
        .map_err(|_| Error::Transport("engine poisoned".into()))?;
    g.learn_peers(&d.peers);
    Ok(g.learned_peers().to_vec())
}

/// Best-effort gossip: describe each seed URL and remember its public peers.
pub async fn gossip_configured_peers(engine: &Mutex<KnotEngine>, seeds: &[String]) {
    for url in seeds {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            learn_from_remote(engine, url),
        )
        .await;
    }
}

pub fn load_or_create_identity(dir: &Path) -> std::io::Result<Identity> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("identity.json");
    if path.exists() {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let ed = decode_seed(v.get("ed25519").and_then(|x| x.as_str()))?;
        let x = decode_seed(v.get("x25519").and_then(|x| x.as_str()))?;
        return Ok(Identity::from_seeds(ed, x));
    }
    let id = Identity::generate();
    let (ed, x) = id.to_seeds();
    let json = serde_json::json!({
        "ed25519": hex::encode(ed),
        "x25519": hex::encode(x),
        "did": id.did_string(),
    });
    let bytes = serde_json::to_vec_pretty(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, bytes)?;
    Ok(id)
}

fn decode_seed(s: Option<&str>) -> std::io::Result<[u8; 32]> {
    let hex_s = s.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "identity.json missing seed",
        )
    })?;
    let bytes = hex::decode(hex_s)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "seed must be 32 bytes"))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn xrpc(State(st): State<AppState>, req: Request<Body>) -> Response {
    if req.method() == Method::OPTIONS {
        return cors_preflight(
            header_str(req.headers(), "origin").as_deref(),
            header_str(req.headers(), "access-control-request-headers").as_deref(),
        );
    }
    let origin = header_str(req.headers(), "origin");
    let has_auth = req.headers().get(header::AUTHORIZATION).is_some()
        || req.headers().get(header::COOKIE).is_some();
    if req.uri().path() == "/xrpc/tangled.sync.listRecords" {
        return from_error(
            Error::Http {
                status: 404,
                error: "MethodNotImplemented".into(),
            },
            has_auth,
            origin.as_deref(),
        );
    }
    let knot_req = match to_knot_request(req).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // Engine dispatch may perform synchronous handle-resolution I/O. Keep it
    // off async workers, and acquire/release its mutex entirely in that task.
    let engine = st.engine.clone();
    let live_clock = st.live_clock;
    let bus = st.bus.clone();
    let (result, knot_req) = match tokio::task::spawn_blocking(move || {
        let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
        if live_clock {
            engine.now = unix_now();
        }
        let result = engine.handle(&knot_req);
        // A queued blocking job survives caller cancellation. Deliver committed
        // events here while the lock still serializes dispatch and event order.
        for event in engine.take_events() {
            let _ = bus.send(event);
        }
        (result, knot_req)
    })
    .await
    {
        Ok(dispatched) => dispatched,
        Err(error) => {
            return from_error(
                Error::Transport(error.to_string()),
                has_auth,
                origin.as_deref(),
            );
        }
    };
    #[cfg(feature = "activitypub")]
    if let Ok(resp) = &result {
        if ap::put_ok(resp) {
            if let Some(record) = ap::put_record_json(&knot_req) {
                ap::after_put_record(&st, &record);
            }
        }
    }
    match result {
        Ok(resp) => from_knot_response(resp, has_auth, origin.as_deref()),
        Err(e) => from_error(e, has_auth, origin.as_deref()),
    }
}

#[cfg(test)]
mod dispatch_cancellation_tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::await_holding_lock)] // Deliberately gate the blocking job until cancellation.
    async fn cancelled_handler_still_broadcasts_its_committed_write() {
        use std::{sync::Arc, time::Duration};
        let identity = Identity::generate();
        let did = identity.did_string();
        let mut engine = KnotEngine::hosting(&did);
        engine.now = 1_700_000_000;
        engine.register_identity(&identity);
        let state = AppState::new(engine, false);
        let mut events = state.bus.subscribe();
        let unsigned = serde_json::json!({
            "v":1, "type":tangled::Cid::dag_cbor(b"pub.tangled.post").to_string(),
            "collection":"pub.tangled.post", "rkey":"cancelled-handler",
            "author":did, "ts":1_700_000_000_000_000u64, "prev":[],
            "vis":"public", "body":{"text":"committed after caller disconnects"},
        });
        let record = tangled::record::sign_json_record(identity.signing_key(), &unsigned)
            .unwrap()
            .json;
        let request = Request::builder()
            .method(Method::POST)
            .uri("/xrpc/tangled.repo.putRecord")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({"record":record})).unwrap(),
            ))
            .unwrap();
        // Block dispatch deterministically until the blocking job owns its Arc.
        let guard = state.engine.lock().unwrap();
        let handler_state = state.clone();
        let handler = tokio::spawn(async move { xrpc(State(handler_state), request).await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while Arc::strong_count(&state.engine) < 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("blocking dispatch job was never queued");
        handler.abort();
        assert!(handler.await.unwrap_err().is_cancelled());
        drop(guard);
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("committed write event was lost after handler cancellation")
            .expect("event bus closed");
        assert_eq!(event.did, did);
        assert!(event
            .ops
            .iter()
            .any(|op| op.collection == "pub.tangled.post" && op.rkey == "cancelled-handler"));
    }
}

async fn to_knot_request(req: Request<Body>) -> std::result::Result<KnotRequest, Response> {
    let method = match *req.method() {
        Method::GET => KnotMethod::Get,
        Method::POST => KnotMethod::Post,
        _ => {
            return Err((StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response());
        }
    };
    let path = req.uri().path().to_string();
    let query = parse_query(req.uri().query());
    let mut headers = Vec::new();
    for (k, v) in req.headers() {
        if let Ok(val) = v.to_str() {
            headers.push((k.as_str().to_string(), val.to_string()));
        }
    }
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = axum::body::to_bytes(req.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE.into_response())?;
    let body = if body.is_empty() {
        None
    } else {
        Some(body.to_vec())
    };
    Ok(KnotRequest {
        method,
        path,
        query,
        headers,
        body,
        content_type,
    })
}

fn parse_query(q: Option<&str>) -> Vec<(String, String)> {
    let Some(q) = q else {
        return Vec::new();
    };
    q.split('&')
        .filter_map(|pair| {
            if pair.is_empty() {
                return None;
            }
            match pair.split_once('=') {
                Some((k, v)) => Some((pct_decode(k), pct_decode(v))),
                None => Some((pct_decode(pair), String::new())),
            }
        })
        .collect()
}

fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("xx"),
                16,
            );
            if let Ok(b) = h {
                out.push(b);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_knot_response(resp: KnotResponse, has_auth: bool, origin: Option<&str>) -> Response {
    let mut builder = Response::builder().status(resp.status);
    if !resp.content_type.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, resp.content_type);
    }
    if let Some(c) = &resp.set_cookie {
        builder = builder.header(header::SET_COOKIE, c);
    }
    for (k, v) in &resp.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    builder = apply_cors(builder, has_auth || resp.set_cookie.is_some(), origin);
    builder
        .body(Body::from(resp.body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn from_error(err: Error, has_auth: bool, origin: Option<&str>) -> Response {
    let (status, error, message) = match err {
        Error::Http { status, error } => {
            let message = error.clone();
            (status, error, message)
        }
        Error::Transport(msg) if msg.contains("websocket") => (
            426,
            "UpgradeRequired".into(),
            "WebSocket upgrade required".into(),
        ),
        other => (500, "Unavailable".into(), other.to_string()),
    };
    let body = serde_json::to_vec(&ApiError {
        error,
        message,
        retry: false,
        head: None,
    })
    .unwrap_or_else(|_| b"{\"error\":\"Unavailable\",\"message\":\"encode\"}".to_vec());
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json");
    builder = apply_cors(builder, has_auth, origin);
    builder
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn apply_cors(
    mut builder: axum::http::response::Builder,
    has_auth: bool,
    origin: Option<&str>,
) -> axum::http::response::Builder {
    builder = builder.header(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        "Authorization, Content-Type, Accept, Cookie",
    );
    if has_auth {
        if let Some(o) = origin {
            builder = builder
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, o)
                .header(header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true")
                .header(header::VARY, "Origin");
        }
    } else {
        builder = builder.header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    }
    builder
}

fn acrh_credentialed(acrh: &str) -> bool {
    acrh.split(',').any(|h| {
        let h = h.trim();
        h.eq_ignore_ascii_case("cookie") || h.eq_ignore_ascii_case("authorization")
    })
}

fn cors_preflight(origin: Option<&str>, request_headers: Option<&str>) -> Response {
    let credentialed = request_headers.is_some_and(acrh_credentialed);
    let mut builder = Response::builder().status(StatusCode::NO_CONTENT).header(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        "Authorization, Content-Type, Accept, Cookie",
    );
    builder = builder.header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS");
    builder = apply_cors(builder, credentialed, origin);
    builder
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[derive(Debug, Deserialize)]
pub(crate) struct SubscribeQuery {
    pub did: String,
    pub collection: Option<String>,
    pub since: Option<String>,
}

pub(crate) async fn socket_loop(
    mut socket: WebSocket,
    st: AppState,
    filter_did: Option<String>,
    collection: Option<String>,
    since: Option<String>,
) {
    let replay = if let (Some(did), Some(since_cid)) = (&filter_did, since.as_deref()) {
        let engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
        engine.events_after(Some(did), since_cid, collection.as_deref())
    } else {
        Vec::new()
    };
    for ev in &replay {
        if send_json(&mut socket, ev).await.is_err() {
            return;
        }
    }
    let hello = if replay.is_empty() {
        if let Some(did) = &filter_did {
            let engine = st.engine.lock().unwrap_or_else(|e| e.into_inner());
            engine.heads.get(did).map(|head| SubscribeMessage {
                t: "commit".into(),
                did: did.clone(),
                commit: head.commit.clone(),
                ops: vec![],
            })
        } else {
            None
        }
    } else {
        None
    };
    if let Some(hello) = hello {
        if send_json(&mut socket, &hello).await.is_err() {
            return;
        }
    }
    let mut rx = st.bus.subscribe();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            ev = rx.recv() => {
                let Ok(ev) = ev else { break; };
                if let Some(did) = &filter_did {
                    if ev.did != *did {
                        continue;
                    }
                    if let Some(c) = &collection {
                        if !ev.ops.is_empty() && ev.ops.iter().all(|op| op.collection != *c) {
                            continue;
                        }
                    }
                }
                if send_json(&mut socket, &ev).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn send_json(socket: &mut WebSocket, msg: &SubscribeMessage) -> Result<(), ()> {
    let text = serde_json::to_string(msg).map_err(|_| ())?;
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|_| ())
}

#[cfg(test)]
mod cors_tests {
    use super::*;

    #[test]
    fn acrh_detects_cookie_and_authorization() {
        assert!(acrh_credentialed("Authorization, Content-Type"));
        assert!(acrh_credentialed("cookie"));
        assert!(!acrh_credentialed("Content-Type, Accept"));
    }
}
