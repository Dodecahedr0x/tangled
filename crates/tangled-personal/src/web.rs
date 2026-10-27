//! Loopback browser application and interoperable owner-authenticated JSON API.
use crate::{execute, Result, Store};
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Json, Router,
};
use rand::RngCore;
use serde_json::{json, Value};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

/// A recovery packet holds hex ciphertext for up to 64 MiB of plaintext.
/// Workflow inputs have a separate 64 MiB bound.
pub const MAX_REQUEST_BYTES: usize = 128 * 1024 * 1024 + 64 * 1024;
const MAX_RESTORES: usize = 4;
const MAX_WORKFLOW_BYTES: usize = 64 * 1024 * 1024;
const FEATURES: &[&str] = &[
    "info",
    "records",
    "memory",
    "capture",
    "notes",
    "feeds",
    "agents",
    "devices",
    "publish",
    "connections",
];
const ASSETS: &[(&str, &str, &str)] = &[
    (
        "/",
        "text/html; charset=utf-8",
        include_str!("../../../apps/personal/web/index.html"),
    ),
    (
        "/index.html",
        "text/html; charset=utf-8",
        include_str!("../../../apps/personal/web/index.html"),
    ),
    (
        "/app.css",
        "text/css; charset=utf-8",
        include_str!("../../../apps/personal/web/app.css"),
    ),
    (
        "/app.js",
        "text/javascript; charset=utf-8",
        include_str!("../../../apps/personal/web/app.js"),
    ),
    (
        "/ui.js",
        "text/javascript; charset=utf-8",
        include_str!("../../../apps/personal/web/ui.js"),
    ),
    (
        "/library.js",
        "text/javascript; charset=utf-8",
        include_str!("../../../apps/personal/web/library.js"),
    ),
    (
        "/reading.js",
        "text/javascript; charset=utf-8",
        include_str!("../../../apps/personal/web/reading.js"),
    ),
    (
        "/manage.js",
        "text/javascript; charset=utf-8",
        include_str!("../../../apps/personal/web/manage.js"),
    ),
];
const SCHEMAS: &[(&str, &str)] = &[
    (
        "pub.tangled.note",
        include_str!("../../../docs/spec/schemas/pub.tangled.note.json"),
    ),
    (
        "pub.tangled.capture",
        include_str!("../../../docs/spec/schemas/pub.tangled.capture.json"),
    ),
    (
        "pub.tangled.savedSearch",
        include_str!("../../../docs/spec/schemas/pub.tangled.savedSearch.json"),
    ),
    (
        "pub.tangled.collection",
        include_str!("../../../docs/spec/schemas/pub.tangled.collection.json"),
    ),
    (
        "pub.tangled.annotation",
        include_str!("../../../docs/spec/schemas/pub.tangled.annotation.memory.json"),
    ),
    (
        "pub.tangled.feedSubscription",
        include_str!("../../../docs/spec/schemas/pub.tangled.feedSubscription.json"),
    ),
    (
        "pub.tangled.feedItem",
        include_str!("../../../docs/spec/schemas/pub.tangled.feedItem.json"),
    ),
    (
        "pub.tangled.readingState",
        include_str!("../../../docs/spec/schemas/pub.tangled.readingState.json"),
    ),
    (
        "pub.tangled.agent.grant",
        include_str!("../../../docs/spec/schemas/pub.tangled.agent.grant.json"),
    ),
    (
        "pub.tangled.agent.activity",
        include_str!("../../../docs/spec/schemas/pub.tangled.agent.activity.json"),
    ),
    (
        "pub.tangled.sourceItem",
        include_str!("../../../docs/spec/schemas/pub.tangled.sourceItem.json"),
    ),
    (
        "pub.tangled.sourceOverlay",
        include_str!("../../../docs/spec/schemas/pub.tangled.sourceOverlay.json"),
    ),
    (
        "pub.tangled.sourceContact",
        include_str!("../../../docs/spec/schemas/pub.tangled.sourceContact.json"),
    ),
    (
        "pub.tangled.replyIntent",
        include_str!("../../../docs/spec/schemas/pub.tangled.replyIntent.json"),
    ),
    (
        "pub.tangled.publication",
        include_str!("../../../docs/spec/schemas/pub.tangled.publication.json"),
    ),
];

struct ActiveStore {
    current: Store,
    // Keep every previously active repository locked until this server stops.
    previous: Vec<Store>,
    initial_data: PathBuf,
}
#[derive(Clone)]
struct WebState {
    store: Arc<Mutex<ActiveStore>>,
    token: Arc<String>,
    authority: String,
    origin: String,
}

/// Bind only to loopback, open and retain one owner Store, and generate a new
/// owner token. The URL is a bootstrap credential; never serve it in HTML.
pub struct Server {
    listener: TcpListener,
    state: WebState,
}
impl Server {
    pub async fn bind(data: impl AsRef<Path>, listen: SocketAddr) -> Result<Self> {
        if !listen.ip().is_loopback() {
            return Err("The personal browser server must listen on a loopback IP address".into());
        }
        let listener = TcpListener::bind(listen)
            .await
            .map_err(|e| format!("Cannot listen: {e}"))?;
        let authority = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let data = data.as_ref().to_path_buf();
        let store = tokio::task::spawn_blocking(move || {
            let current = Store::open(&data)?;
            let initial_data = data.canonicalize().map_err(|e| e.to_string())?;
            Ok::<_, String>(ActiveStore {
                current,
                previous: Vec::new(),
                initial_data,
            })
        })
        .await
        .map_err(|_| "Storage worker stopped")??;
        let mut token = [0; 32];
        rand::thread_rng().fill_bytes(&mut token);
        Ok(Self {
            listener,
            state: WebState {
                store: Arc::new(Mutex::new(store)),
                token: Arc::new(hex::encode(token)),
                origin: format!("http://{authority}"),
                authority,
            },
        })
    }
    pub fn startup_url(&self) -> String {
        format!("{}/#session={}", self.state.origin, self.state.token)
    }
    pub async fn run(self) -> Result<()> {
        let router = Router::new().fallback(any(handle)).with_state(self.state);
        axum::serve(self.listener, router)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .map_err(|e| format!("Browser server failed: {e}"))
    }
}

/// Use the platform browser launcher without interpreting the credential URL as
/// shell code. The caller prints the URL first so launch failure is recoverable.
pub fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Open the printed URL in your browser ({e})"))?;
    Ok(())
}

async fn handle(State(state): State<WebState>, request: Request) -> Response {
    let response = dispatch(state, request).await;
    secure_headers(response)
}

async fn dispatch(state: WebState, request: Request) -> Response {
    if let Err(error) = guard_origin(&state, request.headers()) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let public = path == "/assets/fira-sans.woff2"
        || ASSETS.iter().any(|(p, _, _)| *p == path)
        || path == "/api/v1/manifest"
        || path == "/api/v1/docs"
        || path.starts_with("/api/v1/schemas/");
    if !public && !authorized(request.headers(), &state.token) {
        return failure(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid X-Tangled-Session; open the startup URL printed by this server",
        );
    }
    if public {
        if method != Method::GET && method != Method::HEAD {
            return failure(
                StatusCode::METHOD_NOT_ALLOWED,
                "Use GET for public discovery and application files",
            );
        }
        let mut response = if path == "/assets/fira-sans.woff2" {
            (
                [(header::CONTENT_TYPE, "font/woff2")],
                include_bytes!("../../../apps/reed/assets/fonts/FiraSans-Regular.woff2").as_slice(),
            )
                .into_response()
        } else if let Some((_, mime, contents)) = ASSETS.iter().find(|(p, _, _)| *p == path) {
            ([(header::CONTENT_TYPE, *mime)], *contents).into_response()
        } else if path == "/api/v1/manifest" {
            Json(manifest()).into_response()
        } else if path == "/api/v1/docs" {
            (
                [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
                include_str!("../../../docs/features/personal-app-api.md"),
            )
                .into_response()
        } else if let Some((_, contents)) = SCHEMAS
            .iter()
            .find(|(name, _)| path.strip_prefix("/api/v1/schemas/") == Some(*name))
        {
            (
                [(header::CONTENT_TYPE, "application/schema+json")],
                *contents,
            )
                .into_response()
        } else {
            failure(StatusCode::NOT_FOUND, "Unknown collection schema")
        };
        if method == Method::HEAD {
            *response.body_mut() = Body::empty();
        }
        return response;
    }
    if path == "/api/v1/session" {
        if method != Method::GET {
            return failure(
                StatusCode::METHOD_NOT_ALLOWED,
                "Use GET for session metadata",
            );
        }
        return operation(state, |active| Ok(session(&active.current))).await;
    }
    let feature = path
        .strip_prefix("/api/v1/personal/")
        .filter(|f| FEATURES.contains(f))
        .map(str::to_owned);
    let backup = path
        .strip_prefix("/api/v1/backups/")
        .filter(|a| ["export", "verify", "sync", "restore"].contains(a))
        .map(str::to_owned);
    if feature.is_none() && backup.is_none() {
        return failure(StatusCode::NOT_FOUND, "Unknown API route");
    }
    if method != Method::POST {
        return failure(
            StatusCode::METHOD_NOT_ALLOWED,
            "Use POST with a JSON object",
        );
    }
    let limit = if feature.is_some() {
        MAX_WORKFLOW_BYTES
    } else {
        MAX_REQUEST_BYTES
    };
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|size| size > limit as u64)
    {
        return failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Request exceeds this endpoint's byte limit",
        );
    }
    if !request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|s| {
            s.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
        })
    {
        return failure(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Send Content-Type: application/json",
        );
    }
    let bytes = match to_bytes(request.into_body(), limit).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return failure(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Request exceeds this endpoint's byte limit or could not be read",
            )
        }
    };
    operation(state, move |active| {
        let input: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Invalid JSON request; send a JSON object")?;
        if !input.is_object() { return Err("Invalid JSON request; send a JSON object".into()); }
        if let Some(feature) = feature {
            // HTTP clients exchange files with the browser. Never let an HTTP
            // request choose a server filesystem path through CLI-only actions.
            if feature == "devices" && input["action"] != "status" {
                return Err("Use /api/v1/backups/export, verify, sync or restore with an uploaded packet; server file paths are CLI-only".into());
            }
            if feature == "publish" && input.get("outputDir").is_some() {
                return Err("Download the returned publication files; outputDir is CLI-only".into());
            }
            execute(&feature, &mut active.current, input)
        } else {
            backup_operation(active, backup.as_deref().ok_or("Missing backup action")?, input)
        }
    }).await
}

fn guard_origin(state: &WebState, headers: &HeaderMap) -> std::result::Result<(), &'static str> {
    if headers.get_all(header::HOST).iter().count() != 1
        || headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(state.authority.as_str())
    {
        return Err("Host must exactly match the loopback address printed at startup");
    }
    if headers.contains_key(header::ORIGIN)
        && (headers.get_all(header::ORIGIN).iter().count() != 1
            || headers.get(header::ORIGIN).and_then(|v| v.to_str().ok())
                != Some(state.origin.as_str()))
    {
        return Err("Origin must match this local application");
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| !matches!(v, "same-origin" | "none"))
    {
        return Err("Cross-site requests are not allowed");
    }
    Ok(())
}
fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    if headers.get_all("x-tangled-session").iter().count() != 1 {
        return false;
    }
    let Some(value) = headers.get("x-tangled-session") else {
        return false;
    };
    let provided = value.as_bytes();
    if provided.len() != expected.len() {
        return false;
    }
    provided
        .iter()
        .zip(expected.bytes())
        .fold(0, |diff, (a, b)| diff | (a ^ b))
        == 0
}
fn secure_headers(mut response: Response) -> Response {
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("content-security-policy", "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; font-src 'self'; img-src 'self' data: blob:; frame-src 'self' blob:; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"),
        ("permissions-policy", "camera=(), microphone=(), geolocation=()"),
    ] { response.headers_mut().insert(name, HeaderValue::from_static(value)); }
    response
}
fn failure(status: StatusCode, error: &str) -> Response {
    (status, Json(json!({"error":error}))).into_response()
}
async fn operation(
    state: WebState,
    work: impl FnOnce(&mut ActiveStore) -> Result<Value> + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(move || {
        let result = state
            .store
            .lock()
            .map_err(|_| "Repository worker unavailable; restart the server".to_owned())
            .and_then(|mut active| work(&mut active));
        // Large recovery packets are serialized on this blocking worker too.
        match result {
            Ok(result) => Json(result).into_response(),
            Err(error) => failure(StatusCode::BAD_REQUEST, &error),
        }
    })
    .await
    {
        Ok(response) => response,
        Err(_) => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Repository worker stopped; restart the server",
        ),
    }
}
fn session(store: &Store) -> Value {
    json!({"did":store.did(),"data":store.data_dir(),"version":env!("CARGO_PKG_VERSION"),"apiVersion":"v1","features":FEATURES})
}
fn backup_operation(active: &mut ActiveStore, action: &str, input: Value) -> Result<Value> {
    // tempfile creates an exclusive private directory; only encrypted packets
    // are written here, and drop removes them on both success and failure.
    let transfer =
        tempfile::tempdir().map_err(|_| "Cannot create private recovery staging directory")?;
    let path = transfer.path().join("packet.json");
    let passphrase = input
        .get("passphrase")
        .cloned()
        .ok_or("Missing recovery passphrase")?;
    if action == "export" {
        execute(
            "devices",
            &mut active.current,
            json!({"action":"backup","output_path":path,"passphrase":passphrase}),
        )?;
        let bytes = std::fs::read(&path).map_err(|_| "Cannot read encrypted recovery packet")?;
        let packet: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Cannot encode encrypted recovery packet")?;
        return Ok(json!({"filename":"tangled-recovery.json","backup":packet}));
    }
    let packet = input
        .get("backup")
        .filter(|v| v.is_object())
        .ok_or("Choose an encrypted Tangled recovery JSON file")?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options
        .open(&path)
        .map_err(|_| "Cannot stage encrypted recovery packet")?;
    file.write_all(
        &serde_json::to_vec(packet).map_err(|_| "Cannot encode uploaded recovery packet")?,
    )
    .map_err(|_| "Cannot stage encrypted recovery packet")?;
    drop(file);
    if action != "restore" {
        return execute(
            "devices",
            &mut active.current,
            json!({"action":action,"input_path":path,"passphrase":passphrase}),
        );
    }
    if active.previous.len() >= MAX_RESTORES {
        return Err("This session has restored four libraries; restart with the current session data directory before another restore".into());
    }
    let label = input["label"]
        .as_str()
        .ok_or("Give this restored library a label")?;
    if label.is_empty()
        || label.len() > 80
        || !label
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b' ' | b'-' | b'_'))
    {
        return Err("Use a label of 1–80 letters, numbers, spaces, hyphens or underscores".into());
    }
    let root = active.initial_data.join("restored");
    if !root.exists() {
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&root)
            .map_err(|_| "Cannot create private restored-library directory")?;
    }
    if std::fs::symlink_metadata(&root)
        .map_err(|_| "Cannot inspect restored-library directory")?
        .file_type()
        .is_symlink()
    {
        return Err("Restored-library directory must not be a symbolic link".into());
    }
    let mut suffix = [0; 16];
    rand::thread_rng().fill_bytes(&mut suffix);
    let destination = root.join(format!(
        "{}-{}",
        label.trim().replace(' ', "-"),
        hex::encode(suffix)
    ));
    let restored = execute(
        "devices",
        &mut active.current,
        json!({"action":"restore","input_path":path,"destination":destination,"passphrase":passphrase}),
    )?;
    let next = Store::open(&destination)?;
    active
        .previous
        .push(std::mem::replace(&mut active.current, next));
    Ok(json!({"restored":restored,"session":session(&active.current)}))
}
fn manifest() -> Value {
    let collections: Vec<_> = SCHEMAS
        .iter()
        .map(|(name, _)| json!({"collection":name,"schema":format!("/api/v1/schemas/{name}")}))
        .collect();
    json!({
        "apiVersion":"v1", "version":env!("CARGO_PKG_VERSION"),
        "documentation":"/api/v1/docs", "transport":"loopback HTTP JSON",
        "auth":{"header":"X-Tangled-Session","bootstrap":"owner startup URL fragment #session=HEX","scope":"full local owner authority; signed agent invocations are additionally checked"},
        "limits":{"workflowBytes":MAX_WORKFLOW_BYTES,"backupRequestBytes":MAX_REQUEST_BYTES},
        "endpoints":{"session":"GET /api/v1/session","personal":"POST /api/v1/personal/{feature}","backups":"POST /api/v1/backups/{export,verify,sync,restore}"},
        "collections":collections,
        "actions":{
            "info":[],"records":["list","get","put","delete"],
            "memory":["search","run-search","list-searches","list-collections","delete-search","delete-collection","delete-annotation","save-search","put-collection","annotate"],
            "capture":["save","capture","list","get","read","render","download","delete","update","tag","mark-read","mark-unread","star"],
            "notes":["create","capture","convert","list","due","get","delete","edit","append","attach","attachment"],
            "feeds":["subscribe","list","unsubscribe","ingest","mark","queue","digest","opml"],
            "agents":["grant","revoke","list","report","search","write"],"devices":["status"],
            "publish":["render","export","publish"],
            "connections":["ingest","get","list","search","history","original","select-revision","overlay","contact","contacts","reply-preview","reply-export"]
        }
    })
}
