//! Shared authenticated account archive routes.
mod erasure;
mod pairing;
mod uploads;
use crate::AppState;
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tangled::{identity::Identity, knot::KnotEngine};
use tangled_import::{ArchiveStore, ImportBatch, Receipt, MAX_UPLOAD};

pub type Authenticator = Arc<dyn Fn(&HeaderMap) -> Option<(String, Identity)> + Send + Sync>;
#[derive(Clone)]
pub struct ImportState {
    pub archive: Arc<Mutex<ArchiveStore>>,
    pub engine: Arc<Mutex<KnotEngine>>,
    pub authenticate: Authenticator,
    pub custodial: bool,
}

#[derive(Clone)]
struct UploadGeneration {
    owner: String,
    generation: u64,
}

pub async fn stop_hosted_scope(
    owner: &str,
    source: Option<&str>,
    account: Option<&str>,
) -> Result<(), String> {
    pairing::stop_scope(owner, source, account).await
}

pub async fn hosted_lifecycle_guard() -> tokio::sync::MutexGuard<'static, ()> {
    pairing::lifecycle_guard().await
}
pub async fn stop_hosted_scope_locked(
    owner: &str,
    source: Option<&str>,
    account: Option<&str>,
) -> Result<(), String> {
    pairing::stop_scope_locked(owner, source, account).await
}
type HttpError = (StatusCode, Json<serde_json::Value>);
fn failure(status: StatusCode, message: impl ToString) -> HttpError {
    (status, Json(json!({"error":message.to_string()})))
}
fn bad(message: impl ToString) -> HttpError {
    failure(StatusCode::UNPROCESSABLE_ENTITY, message)
}
fn auth(state: &ImportState, headers: &HeaderMap) -> Result<(String, Identity), HttpError> {
    (state.authenticate)(headers).ok_or_else(|| {
        failure(
            StatusCode::UNAUTHORIZED,
            "Sign in to your Tangled account to open your private archive.",
        )
    })
}
fn same_origin(headers: &HeaderMap) -> Result<(), HttpError> {
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
    {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "Use this account's own host to make changes.",
        ));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().map_err(bad)?;
        let host = headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !["http", "https"]
            .iter()
            .any(|scheme| origin == format!("{scheme}://{host}"))
        {
            return Err(failure(
                StatusCode::FORBIDDEN,
                "Cross-origin account changes are not allowed.",
            ));
        }
    }
    Ok(())
}
pub fn router(state: ImportState) -> Router {
    Router::new()
        .merge(uploads::router())
        .merge(erasure::router())
        .merge(pairing::router())
        .route("/imports", get(page))
        .route("/imports/lifecycle.js", get(lifecycle_script))
        .route("/imports/upload", post(upload_multipart))
        .route("/api/imports", get(list))
        .route("/api/imports/upload", post(upload_json))
        .route("/api/imports/original/{id}", get(original))
        .route("/api/imports/attachment", get(attachment))
        .route("/api/imports/files", get(files))
        .route(
            "/api/imports/export",
            get(export).post(recovery_export_route),
        )
        .route("/api/imports/objects/{id}/delete", post(delete_object))
        .route("/api/imports/connections", post(connect))
        .route("/api/imports/connections/{id}/disconnect", post(disconnect))
        .route("/api/imports/connections/{id}/ingest", post(ingest))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            private_headers,
        ))
        .with_state(state)
}
async fn private_headers(
    State(state): State<ImportState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if request.method() == axum::http::Method::POST
        && ["/imports/upload", "/api/imports/upload"].contains(&request.uri().path())
    {
        if let Some((owner, _)) = (state.authenticate)(request.headers()) {
            let generation = match state
                .archive
                .lock()
                .map_err(bad)
                .and_then(|store| store.erasure_generation(&owner).map_err(bad))
            {
                Ok(generation) => generation,
                Err(error) => return error.into_response(),
            };
            request
                .extensions_mut()
                .insert(UploadGeneration { owner, generation });
        }
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:; media-src 'self' blob:; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'self'"));
    response
}
pub fn self_hosted(state: &AppState) -> Option<Router> {
    let dir = state.engine.lock().ok()?.data_dir.clone()?;
    if !dir.join("identity.json").exists() {
        return None;
    }
    let identity = crate::load_or_create_identity(&dir).ok()?;
    let (ed, x) = identity.to_seeds();
    let did = identity.did_string();
    let archive = Arc::new(Mutex::new(ArchiveStore::open(&dir.join("imports")).ok()?));
    let engine = state.engine.clone();
    let auth_engine = engine.clone();
    let authenticate = Arc::new(move |headers: &HeaderMap| {
        let pairs = headers
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect::<Vec<_>>();
        let token = tangled::session::parse_session_cookie(&pairs)?;
        let engine = auth_engine.lock().ok()?;
        let session = engine.sessions.get(&token)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs();
        if session.did != did || session.exp < now {
            return None;
        }
        Some((did.clone(), Identity::from_seeds(ed, x)))
    });
    Some(router(ImportState {
        archive,
        engine,
        authenticate,
        custodial: false,
    }))
}
async fn page(
    State(state): State<ImportState>,
    headers: HeaderMap,
) -> Result<Html<String>, HttpError> {
    auth(&state, &headers)?;
    let custody = if state.custodial {
        "This provider runs the importer and holds your account keys, so it can read your imported content. Nothing here is published to the public network."
    } else {
        "Your archive is private to your account. Import helpers run with your permission, and this knot holds the local keys needed for unattended imports."
    };
    Ok(Html(
        include_str!("imports.html").replace("__CUSTODY__", custody),
    ))
}

async fn lifecycle_script(
    State(state): State<ImportState>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    auth(&state, &headers)?;
    Ok((
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("imports-lifecycle.js"),
    )
        .into_response())
}
#[derive(Default, Deserialize)]
struct ListQuery {
    source: Option<String>,
    account: Option<String>,
    conversation: Option<String>,
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
}
async fn list(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, HttpError> {
    let (owner, id) = auth(&state, &headers)?;
    let store = state.archive.lock().map_err(bad)?;
    let mut objects = store.objects(&owner, &id).map_err(bad)?;
    objects.retain(|o| {
        query
            .source
            .as_ref()
            .is_none_or(|s| s.is_empty() || o.source == *s)
            && query.account.as_ref().is_none_or(|a| o.account == *a)
            && query
                .conversation
                .as_ref()
                .is_none_or(|c| o.object.conversation.as_ref() == Some(c))
            && query.q.as_ref().is_none_or(|q| {
                serde_json::to_string(&o.object)
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&q.to_lowercase())
            })
    });
    let total = objects.len();
    let objects = objects
        .into_iter()
        .skip(query.offset.unwrap_or(0))
        .take(query.limit.unwrap_or(100).min(500))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"objects":objects,"total":total,"receipts":store.receipts(&owner,&id).map_err(bad)?,
        "connections":store.connections(&owner).map_err(bad)?,"custodial":state.custodial}),
    ))
}
#[derive(Deserialize)]
struct Upload {
    source: String,
    account: String,
    filename: String,
    data_base64: String,
}
#[allow(clippy::too_many_arguments)]
fn import_upload(
    state: &ImportState,
    owner: &str,
    id: &Identity,
    source: &str,
    account: &str,
    filename: &str,
    bytes: &[u8],
    request_generation: u64,
) -> Result<Receipt, HttpError> {
    let generation = state
        .archive
        .lock()
        .map_err(bad)?
        .erasure_generation(owner)
        .map_err(bad)?;
    if generation != request_generation {
        return Err(bad(
            "This upload began before data erasure; start a new upload explicitly.",
        ));
    }
    let batch = tangled_import::parse(source, account, filename, bytes).map_err(bad)?;
    let mut store = state.archive.lock().map_err(bad)?;
    store
        .check_erasure_generation(owner, generation)
        .map_err(bad)?;
    let mut engine = state.engine.lock().map_err(bad)?;
    if engine.signing_keys.get(owner) != Some(&id.to_seeds().0) {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "This host no longer signs for the account.",
        ));
    }
    store.remember_signer(owner, id).map_err(bad)?;
    store
        .ingest(owner, id, &mut engine, filename, bytes, &batch)
        .map_err(bad)
}
async fn upload_json(
    State(state): State<ImportState>,
    headers: HeaderMap,
    snapshot: Option<axum::Extension<UploadGeneration>>,
    Json(payload): Json<Upload>,
) -> Result<Json<Receipt>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    let snapshot = snapshot
        .ok_or_else(|| bad("Missing upload request generation"))?
        .0;
    if snapshot.owner != owner {
        return Err(failure(StatusCode::FORBIDDEN, "Upload owner changed."));
    }
    let raw = STANDARD.decode(&payload.data_base64).map_err(bad)?;
    Ok(Json(import_upload(
        &state,
        &owner,
        &id,
        &payload.source,
        &payload.account,
        &payload.filename,
        &raw,
        snapshot.generation,
    )?))
}
async fn upload_multipart(
    State(state): State<ImportState>,
    headers: HeaderMap,
    snapshot: Option<axum::Extension<UploadGeneration>>,
    mut multipart: Multipart,
) -> Result<Json<Receipt>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    let snapshot = snapshot
        .ok_or_else(|| bad("Missing upload request generation"))?
        .0;
    if snapshot.owner != owner {
        return Err(failure(StatusCode::FORBIDDEN, "Upload owner changed."));
    }
    let mut source = String::new();
    let mut account = String::new();
    let mut filename = String::new();
    let mut data = Vec::new();
    while let Some(mut field) = multipart.next_field().await.map_err(bad)? {
        match field.name().unwrap_or("") {
            "source" => source = field.text().await.map_err(bad)?,
            "account" => account = field.text().await.map_err(bad)?,
            "file" => {
                if !filename.is_empty() {
                    return Err(bad("Import one archive at a time."));
                }
                filename = field.file_name().unwrap_or("archive").to_string();
                while let Some(chunk) = field.chunk().await.map_err(bad)? {
                    if data.len().saturating_add(chunk.len()) > MAX_UPLOAD {
                        return Err(bad("The upload exceeds 256 MiB. Export smaller batches."));
                    }
                    data.extend_from_slice(&chunk);
                }
            }
            _ => {}
        }
    }
    if filename.is_empty() {
        return Err(bad("Choose an export file."));
    }
    Ok(Json(import_upload(
        &state,
        &owner,
        &id,
        &source,
        &account,
        &filename,
        &data,
        snapshot.generation,
    )?))
}
fn download(filename: &str, media_type: &str, bytes: Vec<u8>) -> Response {
    let filename = filename
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || ['.', '_', '-'].contains(c))
        .take(150)
        .collect::<String>();
    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(media_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or(HeaderValue::from_static("attachment")),
    );
    response
}
async fn original(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(batch): Path<String>,
) -> Result<Response, HttpError> {
    let (owner, id) = auth(&state, &headers)?;
    let (name, bytes) = state
        .archive
        .lock()
        .map_err(bad)?
        .original(&owner, &id, &batch)
        .map_err(|_| {
            failure(
                StatusCode::NOT_FOUND,
                "Original export not found in this account.",
            )
        })?;
    Ok(download(&name, "application/octet-stream", bytes))
}
#[derive(Deserialize)]
struct AttachmentQuery {
    source: String,
    account: String,
    name: String,
}
async fn attachment(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Query(query): Query<AttachmentQuery>,
) -> Result<Response, HttpError> {
    let (owner, id) = auth(&state, &headers)?;
    let (mime, bytes) = state
        .archive
        .lock()
        .map_err(bad)?
        .attachment(&owner, &id, &query.source, &query.account, &query.name)
        .map_err(|_| {
            failure(
                StatusCode::NOT_FOUND,
                "This attachment was not supplied or is unavailable.",
            )
        })?;
    Ok(download(&query.name, &mime, bytes))
}

async fn files(
    State(state): State<ImportState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, HttpError> {
    let (owner, id) = auth(&state, &headers)?;
    let files = state
        .archive
        .lock()
        .map_err(bad)?
        .files(&owner, &id)
        .map_err(bad)?;
    Ok(Json(json!({"files":files})))
}
async fn export(
    State(state): State<ImportState>,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let (owner, id) = auth(&state, &headers)?;
    let backup = state
        .archive
        .lock()
        .map_err(bad)?
        .backup(&owner, &id)
        .map_err(bad)?;
    // Key backup is encrypted to the owner's agreement key, not returned in plaintext.
    let bytes = serde_json::to_vec(&backup).map_err(bad)?;
    let (enc, ct) = tangled::hpke::seal_record(
        &id.agreement_public_bytes(),
        b"tangled.import.export.v1",
        &bytes,
    )
    .map_err(bad)?;
    Ok(download("tangled-private-archive.json","application/json",serde_json::to_vec(&json!({"v":1,"owner":owner,"alg":"hpke-record","enc":hex::encode(enc),"ct":hex::encode(ct),"aad":"tangled.import.export.v1"})).map_err(bad)?))
}

#[derive(Deserialize)]
struct RecoveryExportRequest {
    passphrase: String,
}
async fn recovery_export_route(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Json(request): Json<RecoveryExportRequest>,
) -> Result<Response, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    let backup = state
        .archive
        .lock()
        .map_err(bad)?
        .backup(&owner, &id)
        .map_err(bad)?;
    let packet =
        tangled_import::recovery_export(&owner, &id, &backup, &request.passphrase).map_err(bad)?;
    Ok(download(
        "tangled-recovery.json",
        "application/json",
        serde_json::to_vec(&packet).map_err(bad)?,
    ))
}
#[derive(Deserialize)]
struct Connect {
    source: String,
    account: String,
}
async fn connect(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Json(payload): Json<Connect>,
) -> Result<Json<tangled_import::Connected>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    let mut archive = state.archive.lock().map_err(bad)?;
    let engine = state.engine.lock().map_err(bad)?;
    if engine.signing_keys.get(&owner) != Some(&id.to_seeds().0) {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "This host no longer signs for the account.",
        ));
    }
    archive.remember_signer(&owner, &id).map_err(bad)?;
    Ok(Json(
        archive
            .connect(&owner, &payload.source, &payload.account)
            .map_err(bad)?,
    ))
}
async fn disconnect(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(connection): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    same_origin(&headers)?;
    let (owner, _) = auth(&state, &headers)?;
    let source = state
        .archive
        .lock()
        .map_err(bad)?
        .connections(&owner)
        .map_err(bad)?
        .into_iter()
        .find(|c| c.id == connection)
        .ok_or_else(|| failure(StatusCode::NOT_FOUND, "Connection not found."))?;
    let _pairing = pairing::lifecycle_guard().await;
    pairing::stop_scope_locked(&owner, Some(&source.source), Some(&source.account))
        .await
        .map_err(|e| {
            failure(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Hosted authorization could not be removed; retry disconnect: {e}"),
            )
        })?;
    state
        .archive
        .lock()
        .map_err(bad)?
        .disconnect(&owner, &connection)
        .map_err(|_| failure(StatusCode::NOT_FOUND, "Connection not found."))?;
    Ok(Json(
        json!({"disconnected":true,"hosted_authorization_removed":true}),
    ))
}
async fn ingest(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(connection): Path<String>,
    Json(batch): Json<ImportBatch>,
) -> Result<Json<Receipt>, HttpError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            failure(
                StatusCode::UNAUTHORIZED,
                "Provide this connection's ingest token.",
            )
        })?;
    // Keep the connection fence and ingestion under the same lock as disconnect.
    let mut archive = state.archive.lock().map_err(bad)?;
    let connection = archive
        .authorize_ingest(&connection, token)
        .map_err(|e| failure(StatusCode::UNAUTHORIZED, e))?;
    if connection.source != batch.source || connection.account != batch.account {
        return Err(bad("The source/account does not match this connection."));
    }
    let signer = archive.connector_signer(&connection.owner).map_err(bad)?;
    let mut engine = state.engine.lock().map_err(bad)?;
    if engine.signing_keys.get(&connection.owner) != Some(&signer.to_seeds().0) {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "This host no longer signs for the account.",
        ));
    }
    let raw = serde_json::to_vec(&batch).map_err(bad)?;
    let receipt = archive
        .ingest(
            &connection.owner,
            &signer,
            &mut engine,
            "live-batch.json",
            &raw,
            &batch,
        )
        .map_err(bad)?;
    archive.mark_ingested(&connection.id).map_err(bad)?;
    Ok(Json(receipt))
}
async fn delete_object(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(object): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    same_origin(&headers)?;
    let (owner, _) = auth(&state, &headers)?;
    state
        .archive
        .lock()
        .map_err(bad)?
        .delete_object(&owner, &object)
        .map_err(bad)?;
    Ok(Json(json!({"hidden":true,"originals_retained":true})))
}
