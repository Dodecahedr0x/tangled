//! Hosted knot. A person creates an email account; the process holds their
//! `did:sol` and writes public posts for them. `node.describe` sets
//! `custodialSign: true`.

mod accounts;
mod page;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Form, Path as UrlPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use rand::RngCore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tangled::http::IdentityResolveResponse;
use tangled::identity::Identity;
use tangled::record::sign_json_record;
use tangled::Cid;
use tangled::KnotEngine;

use crate::accounts::Account;

const COOKIE: &str = "tangled_account";
const SESSION_SECONDS: i64 = 14 * 24 * 60 * 60;
const POST: &str = "pub.tangled.post";
const PROFILE: &str = "pub.reed.profile";
const SCHEMA: &str = "sys.schema";

#[derive(Debug)]
pub enum AccountError {
    EmailTaken,
    BadLogin,
    Invalid(&'static str),
    Store(String),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmailTaken => write!(f, "That email already has an account."),
            Self::BadLogin => write!(f, "Email or password does not match."),
            Self::Invalid(msg) => write!(f, "{msg}"),
            Self::Store(msg) => write!(f, "{msg}"),
        }
    }
}

pub struct Provider {
    knot: tangled_knot::AppState,
    imports: Arc<Mutex<tangled_import::ArchiveStore>>,
    moves: Mutex<()>,
    db: Mutex<Connection>,
    secure_cookie: bool,
    public_url: String,
}

pub struct FeedPost {
    pub name: String,
    pub text: String,
    pub ts: u64,
}

#[derive(Deserialize)]
struct SignupForm {
    email: String,
    password: String,
    name: String,
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct PostForm {
    text: String,
}

#[derive(Deserialize)]
struct MoveForm {
    knot: String,
    code: String,
    password: String,
}

#[derive(Deserialize)]
struct RotateBody {
    pubkey: String,
    agreement: String,
    knot: String,
    #[serde(default)]
    protocol: u32,
}

#[derive(Serialize)]
struct MoveInfo {
    did: String,
    email: String,
    #[serde(rename = "displayName")]
    display_name: String,
}

#[derive(Serialize, Deserialize)]
struct MoveDone {
    did: String,
    email: String,
    #[serde(rename = "displayName")]
    display_name: String,
    records: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive: Option<SealedMoveArchive>,
}

#[derive(Deserialize)]
struct MoveInfoIn {
    did: String,
    email: String,
}

#[derive(Deserialize)]
struct MoveDoneIn {
    did: String,
    email: String,
    #[serde(rename = "displayName")]
    display_name: String,
    records: Vec<serde_json::Value>,
    #[serde(default)]
    archive: Option<SealedMoveArchive>,
}

#[derive(Serialize, Deserialize)]
struct SealedMoveArchive {
    enc: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct MoveArchive {
    version: u32,
    did: String,
    ed_seed: String,
    x_seed: String,
    backup: tangled_import::ArchiveBackup,
    acknowledgement: String,
}

#[derive(Serialize, Deserialize)]
struct MoveAcknowledgement {
    acknowledgement: String,
}

fn move_binding(did: &str, code: &str, body: &RotateBody) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!([
        "tangled.provider.archive-move.v2",
        did,
        code,
        body.knot,
        body.pubkey.to_lowercase(),
        body.agreement.to_lowercase()
    ]))
    .map_err(|err| err.to_string())
}

#[derive(Deserialize)]
struct RemoteError {
    error: String,
}

impl Provider {
    /// Open or create the knot snapshot and the account database under `data_dir`.
    pub fn open(data_dir: impl AsRef<Path>, public_url: &str) -> Result<Arc<Self>, String> {
        let data_dir = data_dir.as_ref();
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let public_url = public_url.trim_end_matches('/').to_string();
        let operator = load_or_create_operator(data_dir)?;
        let mut engine =
            KnotEngine::open(data_dir, &operator.did_string()).map_err(|e| e.to_string())?;
        engine = engine.with_knot_base(public_url.clone());
        if let Some(describe) = engine.describe.as_mut() {
            describe.custodial_sign = true;
        }
        engine.now = unix_now();
        engine.register_identity(&operator);
        let db = Connection::open(data_dir.join("accounts.db")).map_err(|e| e.to_string())?;
        restrict_file(&data_dir.join("accounts.db"));
        accounts::migrate(&db).map_err(|e| e.to_string())?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS archive_moves (
            code TEXT PRIMARY KEY, account_id INTEGER NOT NULL, binding TEXT NOT NULL,
            response TEXT NOT NULL, acknowledgement TEXT NOT NULL, acknowledged INTEGER NOT NULL DEFAULT 0
        ); CREATE TABLE IF NOT EXISTS archive_move_receipts (
            code TEXT NOT NULL, knot TEXT NOT NULL, acknowledgement TEXT NOT NULL,
            PRIMARY KEY(code,knot)
        );").map_err(|e| e.to_string())?;
        let imports = Arc::new(Mutex::new(tangled_import::ArchiveStore::open(
            &data_dir.join("imports"),
        )?));
        for account in accounts::all_seeds(&db).map_err(|e| e.to_string())? {
            engine
                .signing_keys
                .insert(account.did.clone(), account.ed_seed);
            bind_identity(&mut engine, &account.did, &public_url);
        }
        engine.persist().map_err(|e| e.to_string())?;
        let knot = tangled_knot::AppState::new(engine, true);
        Ok(Arc::new(Self {
            knot,
            imports,
            moves: Mutex::new(()),
            db: Mutex::new(db),
            secure_cookie: public_url.starts_with("https://"),
            public_url,
        }))
    }
}

pub fn router(provider: Arc<Provider>) -> Router {
    let knot = tangled_knot::router(provider.knot.clone());
    let auth_provider = Arc::clone(&provider);
    let imports = tangled_knot::imports::router(tangled_knot::imports::ImportState {
        archive: Arc::clone(&provider.imports),
        engine: Arc::clone(&provider.knot.engine),
        authenticate: Arc::new(move |headers| {
            let account = session_account(&auth_provider, headers)?;
            if !accounts::signing_allowed(&lock_db(&auth_provider), &account.did).ok()? {
                return None;
            }
            let id = Identity::from_seeds(account.ed_seed, account.x_seed);
            Some((account.did, id))
        }),
        custodial: true,
    });
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/", get(home))
        .route("/signup", post(signup))
        .route("/login", get(login_form).post(login))
        .route("/logout", post(logout))
        .route("/posts", post(create_post))
        .route("/move", get(move_page).post(accept_move))
        .route("/migrate/start", post(start_move))
        .route("/migrate/{code}", get(move_info).post(finish_move))
        .route("/migrate/{code}/ack", post(acknowledge_move))
        .fallback_service(knot)
        .with_state(provider)
        .merge(imports)
}

async fn home(State(p): State<Arc<Provider>>, headers: HeaderMap) -> Response {
    let posts = feed(&p);
    if let Some(account) = session_account(&p, &headers) {
        return Html(page::home(
            &account.display_name,
            &account.did,
            None,
            &posts,
        ))
        .into_response();
    }
    Html(page::welcome(None, &posts)).into_response()
}

async fn login_form() -> Html<String> {
    Html(page::login(None))
}

async fn signup(State(p): State<Arc<Provider>>, Form(form): Form<SignupForm>) -> Response {
    match create_account(&p, &form) {
        Ok(token) => signed_in_redirect(&p, &token),
        Err(err) => {
            let posts = feed(&p);
            (
                StatusCode::BAD_REQUEST,
                Html(page::welcome_with_values(
                    Some(&err.to_string()),
                    &posts,
                    &form.name,
                    &form.email,
                )),
            )
                .into_response()
        }
    }
}

async fn login(State(p): State<Arc<Provider>>, Form(form): Form<LoginForm>) -> Response {
    let email = normalize_email(&form.email);
    let account = {
        let db = lock_db(&p);
        accounts::login(&db, &email, &form.password)
    };
    match account {
        Ok(account) => {
            let token = new_token();
            let exp = unix_now() as i64 + SESSION_SECONDS;
            if let Err(err) = accounts::insert_session(&lock_db(&p), &token, account.id, exp) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html(page::login_with_email(Some(&err.to_string()), &form.email)),
                )
                    .into_response();
            }
            signed_in_redirect(&p, &token)
        }
        Err(err) => (
            StatusCode::UNAUTHORIZED,
            Html(page::login_with_email(Some(&err.to_string()), &form.email)),
        )
            .into_response(),
    }
}

async fn logout(State(p): State<Arc<Provider>>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_token(&headers) {
        let _ = accounts::delete_session(&lock_db(&p), &token);
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!("{COOKIE}=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax")
            .parse()
            .expect("clear cookie"),
    );
    (headers, Redirect::to("/")).into_response()
}

async fn create_post(
    State(p): State<Arc<Provider>>,
    headers: HeaderMap,
    Form(form): Form<PostForm>,
) -> Response {
    let Some(account) = session_account(&p, &headers) else {
        return Redirect::to("/login").into_response();
    };
    let text = form.text.trim();
    if text.is_empty() || text.len() > 2000 {
        let posts = feed(&p);
        return Html(page::home_with_draft(
            &account.display_name,
            &account.did,
            Some("Write a post, up to 2000 characters."),
            &posts,
            &form.text,
        ))
        .into_response();
    }
    let id = Identity::from_seeds(account.ed_seed, account.x_seed);
    if let Err(err) = publish(
        &p,
        &account.did,
        &id,
        POST,
        &tangled::identity::tid_now(),
        serde_json::json!({ "text": text }),
    ) {
        let posts = feed(&p);
        return (
            StatusCode::BAD_GATEWAY,
            Html(page::home_with_draft(
                &account.display_name,
                &account.did,
                Some(&err),
                &posts,
                &form.text,
            )),
        )
            .into_response();
    }
    Redirect::to("/").into_response()
}

fn create_account(p: &Provider, form: &SignupForm) -> Result<String, AccountError> {
    let email = normalize_email(&form.email);
    validate_email(&email)?;
    let name = form.name.trim();
    if name.is_empty() || name.chars().count() > 40 || name.contains(['\n', '\r']) {
        return Err(AccountError::Invalid("Use a name, up to 40 characters."));
    }
    if form.password.len() < 8 || form.password.len() > 200 {
        return Err(AccountError::Invalid(
            "Use a password of at least 8 characters.",
        ));
    }
    {
        let db = lock_db(p);
        if accounts::by_email(&db, &email)?.is_some() {
            return Err(AccountError::EmailTaken);
        }
    }
    let id = Identity::generate();
    let (ed, x) = id.to_seeds();
    let did = id.did_string();
    publish(
        p,
        &did,
        &id,
        SCHEMA,
        &tangled::identity::tid_now(),
        serde_json::json!({
            "v": 1,
            "name": PROFILE,
            "fields": {
                "displayName": { "type": "string" },
                "bio": { "type": "string" }
            }
        }),
    )
    .map_err(AccountError::Store)?;
    publish(
        p,
        &did,
        &id,
        PROFILE,
        "self",
        serde_json::json!({ "displayName": name, "bio": "" }),
    )
    .map_err(AccountError::Store)?;
    let new = accounts::NewAccount {
        email: email.clone(),
        display_name: name.to_string(),
        did: did.clone(),
        ed_seed: ed,
        x_seed: x,
    };
    let account_id =
        accounts::insert_account(&lock_db(p), &new, &form.password, unix_now() as i64)?;
    let token = new_token();
    accounts::insert_session(
        &lock_db(p),
        &token,
        account_id,
        unix_now() as i64 + SESSION_SECONDS,
    )?;
    Ok(token)
}

fn publish(
    p: &Provider,
    author: &str,
    id: &Identity,
    collection: &str,
    rkey: &str,
    body: serde_json::Value,
) -> Result<(), String> {
    if !accounts::signing_allowed(&lock_db(p), author).map_err(|e| e.to_string())? {
        return Err("This host can no longer sign for this account.".into());
    }
    let unsigned = serde_json::json!({
        "v": 1,
        "type": Cid::dag_cbor(collection.as_bytes()).to_string(),
        "collection": collection,
        "rkey": rkey,
        "author": author,
        "ts": unix_micros(),
        "prev": [],
        "vis": "public",
        "body": body,
    });
    let signed = sign_json_record(id.signing_key(), &unsigned).map_err(|e| e.to_string())?;
    let mut engine = p.knot.engine.lock().unwrap_or_else(|e| e.into_inner());
    // Recheck after taking the same engine lock as move cutover: an already
    // authenticated request must not publish after the migration snapshot.
    if !accounts::signing_allowed(&lock_db(p), author).map_err(|e| e.to_string())? {
        return Err("This host can no longer sign for this account.".into());
    }
    engine.now = unix_now();
    engine
        .signing_keys
        .insert(author.to_string(), id.to_seeds().0);
    let url = engine.knot_base.clone().unwrap_or_default();
    bind_identity(&mut engine, author, &url);
    engine
        .ingest_record(signed.json)
        .map_err(|e| e.to_string())?;
    let _ = engine.take_events();
    Ok(())
}

const MOVE_SECONDS: i64 = 15 * 60;
const SYS_ID: &str = "sys.id";

async fn move_page(State(p): State<Arc<Provider>>, headers: HeaderMap) -> Response {
    if let Some(account) = session_account(&p, &headers) {
        let code = accounts::active_code(&lock_db(&p), account.id, unix_now() as i64);
        return match code {
            Ok(Some(code)) => Html(page::move_code(&code, &p.public_url)).into_response(),
            Ok(None) => Html(page::move_start(None)).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(page::move_start(Some(&err.to_string()))),
            )
                .into_response(),
        };
    }
    Html(page::move_in(None)).into_response()
}

async fn start_move(State(p): State<Arc<Provider>>, headers: HeaderMap) -> Response {
    let Some(account) = session_account(&p, &headers) else {
        return Redirect::to("/login").into_response();
    };
    if !accounts::signing_allowed(&lock_db(&p), &account.did).unwrap_or(false) {
        return Html(page::move_stopped()).into_response();
    }
    let code = new_token()[..32].to_string();
    let exp = unix_now() as i64 + MOVE_SECONDS;
    if let Err(err) = accounts::start_move(&lock_db(&p), account.id, &code, exp) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(page::move_start(Some(&err.to_string()))),
        )
            .into_response();
    }
    Redirect::to("/move").into_response()
}

async fn move_info(State(p): State<Arc<Provider>>, UrlPath(code): UrlPath<String>) -> Response {
    let Some(info) = load_move(&p, &code) else {
        return json_error(StatusCode::NOT_FOUND, "That move code is not valid.");
    };
    Json(MoveInfo {
        did: info.account.did,
        email: info.email,
        display_name: info.account.display_name,
    })
    .into_response()
}

async fn finish_move(
    State(p): State<Arc<Provider>>,
    UrlPath(code): UrlPath<String>,
    Json(body): Json<RotateBody>,
) -> Response {
    let _pairing_guard = tangled_knot::imports::hosted_lifecycle_guard().await;
    // Source sessions are provider-local custody and never migrate. Stop and
    // remove hosted workers before acquiring synchronous migration/store locks.
    if body.protocol == 2
        && decode_key(&body.pubkey).is_ok()
        && decode_key(&body.agreement).is_ok()
        && host_url(&body.knot).is_ok()
    {
        if let Some(info) = load_move(&p, &code) {
            if let Err(error) =
                tangled_knot::imports::stop_hosted_scope_locked(&info.account.did, None, None).await
            {
                return json_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    &format!("Could not retire hosted source authorization: {error}"),
                );
            }
        }
    }
    let _move_guard = p.moves.lock().unwrap_or_else(|e| e.into_inner());
    if body.protocol != 2 {
        return json_error(
            StatusCode::BAD_REQUEST,
            "This host requires the encrypted, acknowledged move protocol (version 2).",
        );
    }
    let Some(info) = load_move(&p, &code) else {
        return json_error(StatusCode::NOT_FOUND, "That move code is not valid.");
    };
    if decode_key(&body.pubkey).is_err() {
        return json_error(StatusCode::BAD_REQUEST, "pubkey must be 32 bytes hex.");
    }
    let Ok(agreement) = decode_key(&body.agreement) else {
        return json_error(StatusCode::BAD_REQUEST, "agreement must be 32 bytes hex.");
    };
    let Ok(knot) = host_url(&body.knot) else {
        return json_error(StatusCode::BAD_REQUEST, "knot must be an http(s) URL.");
    };
    if knot != body.knot {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Use the canonical destination URL.",
        );
    }
    let binding = match move_binding(&info.account.did, &code, &body) {
        Ok(binding) => binding,
        Err(err) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err),
    };
    let prepared: Result<(String, String), _> = lock_db(&p).query_row(
        "SELECT binding,response FROM archive_moves WHERE code=?1",
        [&code],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    match prepared {
        Ok((previous, response)) => {
            if previous != binding {
                return json_error(
                    StatusCode::CONFLICT,
                    "This move is already bound to another destination.",
                );
            }
            return ([(header::CONTENT_TYPE, "application/json")], response).into_response();
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {}
        Err(err) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    }
    if info.used || !accounts::signing_allowed(&lock_db(&p), &info.account.did).unwrap_or(false) {
        return json_error(StatusCode::CONFLICT, "That move code was already used.");
    }
    let id = Identity::from_seeds(info.account.ed_seed, info.account.x_seed);
    let mut archive = p.imports.lock().unwrap_or_else(|e| e.into_inner());
    // Fencing and the backup use the same archive lock as ingestion. No worker
    // can commit after disconnect, and destination connections need fresh authorization.
    if let Err(err) = archive.disconnect_all(&info.account.did) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err);
    }
    let backup = match archive.backup(&info.account.did, &id) {
        Ok(backup) => backup,
        Err(err) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err),
    };
    let acknowledgement = new_token();
    let handoff = MoveArchive {
        version: 2,
        did: info.account.did.clone(),
        ed_seed: hex::encode(info.account.ed_seed),
        x_seed: hex::encode(info.account.x_seed),
        backup,
        acknowledgement: acknowledgement.clone(),
    };
    let plaintext = match serde_json::to_vec(&handoff) {
        Ok(bytes) => bytes,
        Err(err) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    };
    let (enc, ciphertext) =
        match tangled::hpke::seal_record(&agreement, binding.as_bytes(), &plaintext) {
            Ok(sealed) => sealed,
            Err(err) => return json_error(StatusCode::BAD_REQUEST, &err.to_string()),
        };
    let mut engine = p.knot.engine.lock().unwrap_or_else(|e| e.into_inner());
    engine.now = unix_now();
    // Location moves preserve operational keys so all prior private records and
    // agreement-key epochs remain decryptable and retain their valid signatures.
    let unsigned = serde_json::json!({
        "v":1, "type":Cid::dag_cbor(SYS_ID.as_bytes()).to_string(),
        "collection":SYS_ID, "rkey":"self", "author":info.account.did,
        "ts":unix_micros(), "prev":[], "vis":"public",
        "body":{
            "v":1, "pubkey":hex::encode(id.pubkey_bytes()),
            "agreement":hex::encode(id.agreement_public_bytes()),
            "rotation":[hex::encode(id.pubkey_bytes())], "recovery":[],
            "endpoints":[{"type":"knot","url":knot}], "alsoKnownAs":[], "bindings":[]
        }
    });
    let signed = match sign_json_record(id.signing_key(), &unsigned) {
        Ok(signed) => signed,
        Err(err) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    };
    if let Err(err) = engine.ingest_record(signed.json) {
        return json_error(StatusCode::BAD_GATEWAY, &err.to_string());
    }
    let mut records: Vec<_> = engine
        .stored
        .iter()
        .filter(|record| record.did == info.account.did && record.vis == "public")
        .map(|record| record.record.clone())
        .collect();
    records.sort_by_key(|record| record.get("ts").and_then(|ts| ts.as_u64()).unwrap_or(0));
    let done = MoveDone {
        did: info.account.did.clone(),
        email: info.email,
        display_name: info.account.display_name,
        records,
        archive: Some(SealedMoveArchive {
            enc: hex::encode(enc),
            ciphertext: hex::encode(ciphertext),
        }),
    };
    let response = match serde_json::to_string(&done) {
        Ok(response) => response,
        Err(err) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()),
    };
    if let Err(err) = engine.persist() {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    }
    let db = lock_db(&p);
    let result = (|| -> Result<(), rusqlite::Error> {
        let tx = db.unchecked_transaction()?;
        tx.execute("INSERT INTO archive_moves(code,account_id,binding,response,acknowledgement) VALUES(?1,?2,?3,?4,?5)",
            rusqlite::params![code,info.account.id,binding,response,acknowledgement])?;
        // Keep the source seeds only for recovery until the destination proves
        // it decrypted and durably stored the handoff. Signing is already fenced.
        tx.execute(
            "UPDATE accounts SET custodial=0 WHERE id=?1",
            [info.account.id],
        )?;
        tx.execute("UPDATE moves SET used=1 WHERE code=?1", [&code])?;
        tx.commit()
    })();
    if let Err(err) = result {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    }
    engine.signing_keys.remove(&info.account.did);
    Json(done).into_response()
}

async fn acknowledge_move(
    State(p): State<Arc<Provider>>,
    UrlPath(code): UrlPath<String>,
    Json(body): Json<MoveAcknowledgement>,
) -> Response {
    let _move_guard = p.moves.lock().unwrap_or_else(|e| e.into_inner());
    let db = lock_db(&p);
    let prepared: Result<(i64, String), _> = db.query_row(
        "SELECT account_id,acknowledgement FROM archive_moves WHERE code=?1",
        [&code],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    let Ok((account_id, expected)) = prepared else {
        return json_error(StatusCode::NOT_FOUND, "Prepared move not found.");
    };
    if body.acknowledgement != expected {
        return json_error(
            StatusCode::FORBIDDEN,
            "Destination decryption proof is invalid.",
        );
    }
    let result = (|| -> Result<(), String> {
        let tx = db.unchecked_transaction().map_err(|err| err.to_string())?;
        accounts::wipe_custodial(&tx, account_id, &code).map_err(|err| err.to_string())?;
        tx.execute(
            "UPDATE archive_moves SET acknowledged=1 WHERE code=?1",
            [&code],
        )
        .map_err(|err| err.to_string())?;
        tx.commit().map_err(|err| err.to_string())
    })();
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &err),
    }
}

async fn accept_move(State(p): State<Arc<Provider>>, Form(form): Form<MoveForm>) -> Response {
    let fail =
        |msg: String| (StatusCode::BAD_REQUEST, Html(page::move_in(Some(&msg)))).into_response();
    if form.password.len() < 8 || form.password.len() > 200 {
        return fail("Use a password of at least 8 characters.".into());
    }
    let Ok(knot) = host_url(&form.knot) else {
        return fail("Enter the current host’s URL.".into());
    };
    let code = form.code.trim().to_ascii_lowercase();
    if !valid_code(&code) {
        return fail("Enter the move code from the current host.".into());
    }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(err) => return fail(err.to_string()),
    };
    let info = match client.get(format!("{knot}/migrate/{code}")).send().await {
        Ok(resp) => match read_remote::<MoveInfoIn>(resp).await {
            Ok(info) => info,
            Err(err) => return fail(err),
        },
        Err(_) => return fail("Could not reach the current host.".into()),
    };
    let existing_account = accounts::by_did(&lock_db(&p), &info.did).ok().flatten();
    if let Some(account) = existing_account {
        let receipt: Option<String> = lock_db(&p)
            .query_row(
                "SELECT acknowledgement FROM archive_move_receipts WHERE code=?1 AND knot=?2",
                rusqlite::params![code, knot],
                |row| row.get(0),
            )
            .ok();
        if let Some(acknowledgement) = receipt {
            if accounts::login(&lock_db(&p), &info.email, &form.password).is_err() {
                return fail("Use the password you chose for the resumed move.".into());
            }
            if let Err(err) = send_move_ack(&client, &knot, &code, acknowledgement).await {
                return fail(err);
            }
            clear_incoming_move(&p, &knot, &code);
            return moved_session(&p, account.id);
        }
        return Redirect::to("/login").into_response();
    }
    if accounts::by_email(&lock_db(&p), &normalize_email(&info.email))
        .ok()
        .flatten()
        .is_some()
    {
        return fail("That email already has an account on this host.".into());
    }
    // Select/create the temporary recipient under one database lock so concurrent
    // retries cannot replace the only key that can decrypt the source handoff.
    let (temporary_ed, temporary_x) = {
        let db = lock_db(&p);
        match accounts::incoming_key(&db, &code, &knot) {
            Ok(Some(keys)) => keys,
            Ok(None) => {
                let keys = Identity::generate().to_seeds();
                if let Err(err) = accounts::save_incoming(&db, &code, &knot, &keys.0, &keys.1) {
                    return fail(err.to_string());
                }
                keys
            }
            Err(err) => return fail(err.to_string()),
        }
    };
    let temporary = Identity::from_seeds(temporary_ed, temporary_x);
    let request = RotateBody {
        pubkey: hex::encode(temporary.pubkey_bytes()),
        agreement: hex::encode(temporary.agreement_public_bytes()),
        knot: p.public_url.clone(),
        protocol: 2,
    };
    let done = match client
        .post(format!("{knot}/migrate/{code}"))
        .json(&serde_json::json!({
            "pubkey":request.pubkey, "agreement":request.agreement,
            "knot":request.knot, "protocol":request.protocol
        }))
        .send()
        .await
    {
        Ok(resp) => match read_remote::<MoveDoneIn>(resp).await {
            Ok(done) => done,
            Err(err) => return fail(err),
        },
        Err(_) => {
            return fail(
                "The current host did not finish preparing the move. Retry with the same code."
                    .into(),
            )
        }
    };
    if done.did != info.did || normalize_email(&done.email) != normalize_email(&info.email) {
        return fail("The moved account did not match the move invitation.".into());
    }
    let handoff = if let Some(sealed) = &done.archive {
        let opened = (|| -> Result<MoveArchive, String> {
            let binding = move_binding(&done.did, &code, &request)?;
            let enc = hex::decode(&sealed.enc).map_err(|err| err.to_string())?;
            let ciphertext = hex::decode(&sealed.ciphertext).map_err(|err| err.to_string())?;
            let bytes =
                tangled::hpke::open_record(&temporary_x, &enc, binding.as_bytes(), &ciphertext)
                    .map_err(|err| err.to_string())?;
            let handoff: MoveArchive =
                serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
            if handoff.version != 2 || handoff.did != done.did || handoff.backup.owner != done.did {
                return Err("Encrypted archive handoff owner/version mismatch.".into());
            }
            if handoff.acknowledgement.len() < 32 {
                return Err("Invalid destination acknowledgement challenge.".into());
            }
            Ok(handoff)
        })();
        match opened {
            Ok(handoff) => Some(handoff),
            Err(err) => return fail(format!("Could not decrypt the archive handoff: {err}")),
        }
    } else {
        // Legacy sources only support public history and rotate to these temporary keys.
        None
    };
    let (ed, x) = if let Some(handoff) = &handoff {
        match (decode_key(&handoff.ed_seed), decode_key(&handoff.x_seed)) {
            (Ok(ed), Ok(x)) => (ed, x),
            _ => return fail("Invalid transferred identity keys.".into()),
        }
    } else {
        (temporary_ed, temporary_x)
    };
    let identity = Identity::from_seeds(ed, x);
    let persisted = (|| -> Result<(), String> {
        let mut archive = p.imports.lock().unwrap_or_else(|e| e.into_inner());
        let mut engine = p.knot.engine.lock().unwrap_or_else(|e| e.into_inner());
        engine.now = unix_now();
        engine.signing_keys.insert(done.did.clone(), ed);
        bind_identity(&mut engine, &done.did, &p.public_url);
        for record in &done.records {
            if record["author"].as_str() != Some(&done.did) {
                return Err("Move contained another account's history.".into());
            }
            if !already_stored(&engine, record) {
                engine
                    .ingest_record(record.clone())
                    .map_err(|err| format!("Could not store the moved history: {err}"))?;
            }
        }
        if let Some(handoff) = &handoff {
            let tip = engine
                .tips
                .get(&(done.did.clone(), SYS_ID.into(), "self".into()))
                .and_then(|cid| engine.stored.iter().find(|record| &record.cid == cid))
                .ok_or("Moved identity record is missing.")?;
            if tip.record["body"]["pubkey"].as_str() != Some(&hex::encode(identity.pubkey_bytes()))
                || tip.record["body"]["agreement"].as_str()
                    != Some(&hex::encode(identity.agreement_public_bytes()))
            {
                return Err("Transferred custody keys do not match the signed identity.".into());
            }
            archive.restore(&done.did, &identity, &handoff.backup)?;
            archive.project_backup(&done.did, &handoff.backup, &mut engine)?;
            // Validate every locally retained manifest and blob. Foreign attachment
            // references can name files the source export explicitly omitted.
            archive.verify_archive(&done.did, &identity)?;
        }
        engine.signing_keys.insert(done.did.clone(), ed);
        engine.persist().map_err(|err| err.to_string())
    })();
    if let Err(err) = persisted {
        return fail(err);
    }
    let new = accounts::NewAccount {
        email: normalize_email(&done.email),
        display_name: done.display_name,
        did: done.did,
        ed_seed: ed,
        x_seed: x,
    };
    let account_id = {
        let db = lock_db(&p);
        let persist_account = (|| -> Result<i64, String> {
            let tx = db.unchecked_transaction().map_err(|err| err.to_string())?;
            let account_id = accounts::insert_account(&tx, &new, &form.password, unix_now() as i64)
                .map_err(|err| err.to_string())?;
            if let Some(handoff) = &handoff {
                tx.execute(
                    "INSERT OR REPLACE INTO archive_move_receipts VALUES(?1,?2,?3)",
                    rusqlite::params![code, knot, handoff.acknowledgement],
                )
                .map_err(|err| err.to_string())?;
            }
            tx.commit().map_err(|err| err.to_string())?;
            Ok(account_id)
        })();
        match persist_account {
            Ok(account_id) => account_id,
            Err(err) => return fail(err),
        }
    };
    if let Some(handoff) = handoff {
        if let Err(err) = send_move_ack(&client, &knot, &code, handoff.acknowledgement).await {
            return fail(err);
        }
    }
    clear_incoming_move(&p, &knot, &code);
    moved_session(&p, account_id)
}

async fn send_move_ack(
    client: &reqwest::Client,
    knot: &str,
    code: &str,
    acknowledgement: String,
) -> Result<(), String> {
    let response = client.post(format!("{knot}/migrate/{code}/ack"))
        .json(&MoveAcknowledgement { acknowledgement }).send().await
        .map_err(|_| "The archive is saved here, but the old host did not acknowledge retirement. Retry this move with the same code and password.".to_string())?;
    if !response.status().is_success() {
        return Err("The archive is saved here, but the old host did not retire custody. Retry this move with the same code and password.".into());
    }
    Ok(())
}

fn clear_incoming_move(p: &Provider, knot: &str, code: &str) {
    let db = lock_db(p);
    let _ = accounts::delete_incoming(&db, code, knot);
    let _ = db.execute(
        "DELETE FROM archive_move_receipts WHERE code=?1 AND knot=?2",
        rusqlite::params![code, knot],
    );
}

fn moved_session(p: &Provider, account_id: i64) -> Response {
    let token = new_token();
    if let Err(err) = accounts::insert_session(
        &lock_db(p),
        &token,
        account_id,
        unix_now() as i64 + SESSION_SECONDS,
    ) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
    }
    signed_in_redirect(p, &token)
}

fn load_move(p: &Provider, code: &str) -> Option<accounts::MoveAccount> {
    if !valid_code(code) {
        return None;
    }
    accounts::move_account(&lock_db(p), &code.to_ascii_lowercase(), unix_now() as i64)
        .ok()
        .flatten()
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

async fn read_remote<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> Result<T, String> {
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let message = serde_json::from_slice::<RemoteError>(&bytes)
            .map(|e| e.error)
            .unwrap_or_else(|_| "The current host refused the move.".into());
        return Err(message);
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn already_stored(engine: &KnotEngine, rec: &serde_json::Value) -> bool {
    let Some(author) = rec.get("author").and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(collection) = rec.get("collection").and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(rkey) = rec.get("rkey").and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(sig) = rec.get("sig").and_then(|v| v.as_str()) else {
        return false;
    };
    engine.stored.iter().any(|r| {
        r.did == author
            && r.collection == collection
            && r.rkey == rkey
            && r.record.get("sig").and_then(|s| s.as_str()) == Some(sig)
    })
}

fn valid_code(code: &str) -> bool {
    code.len() == 32 && code.bytes().all(|b| b.is_ascii_hexdigit())
}

fn decode_key(hex_s: &str) -> Result<[u8; 32], ()> {
    let bytes = hex::decode(hex_s.trim()).map_err(|_| ())?;
    bytes.try_into().map_err(|_| ())
}

fn host_url(raw: &str) -> Result<String, ()> {
    let url = raw.trim().trim_end_matches('/').to_string();
    let parsed = reqwest::Url::parse(&url).map_err(|_| ())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(());
    }
    if parsed.host_str().is_none() {
        return Err(());
    }
    Ok(url)
}

fn feed(p: &Provider) -> Vec<FeedPost> {
    let names: HashMap<String, String> = accounts::names(&lock_db(p))
        .unwrap_or_default()
        .into_iter()
        .collect();
    let engine = p.knot.engine.lock().unwrap_or_else(|e| e.into_inner());
    let mut posts = Vec::new();
    for rec in &engine.stored {
        if rec.collection != POST || rec.vis != "public" || rec.deleted {
            continue;
        }
        let tip = engine
            .tips
            .get(&(rec.did.clone(), rec.collection.clone(), rec.rkey.clone()));
        if tip != Some(&rec.cid) {
            continue;
        }
        let Some(text) = rec
            .record
            .get("body")
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
        else {
            continue;
        };
        let ts = rec.record.get("ts").and_then(|t| t.as_u64()).unwrap_or(0);
        let name = names
            .get(&rec.did)
            .cloned()
            .unwrap_or_else(|| short_did(&rec.did));
        posts.push(FeedPost {
            name,
            text: text.to_string(),
            ts,
        });
    }
    posts.sort_by_key(|post| std::cmp::Reverse(post.ts));
    posts
}

fn bind_identity(engine: &mut KnotEngine, did: &str, url: &str) {
    let id_cid = engine.identities.get(did).and_then(|i| i.id_cid.clone());
    engine.identities.insert(
        did.to_string(),
        IdentityResolveResponse {
            did: did.to_string(),
            id_cid,
            endpoints: vec![url.to_string()],
            document: None,
        },
    );
}

fn session_account(p: &Provider, headers: &HeaderMap) -> Option<Account> {
    let token = cookie_token(headers)?;
    accounts::account_for_session(&lock_db(p), &token, unix_now() as i64).ok()?
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&format!("{COOKIE}=")) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn signed_in_redirect(p: &Provider, token: &str) -> Response {
    let secure = if p.secure_cookie { "; Secure" } else { "" };
    let cookie = format!(
        "{COOKIE}={token}; HttpOnly; Path=/; Max-Age={SESSION_SECONDS}; SameSite=Lax{secure}"
    );
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().expect("session cookie"));
    (headers, Redirect::to("/")).into_response()
}

fn lock_db(p: &Provider) -> std::sync::MutexGuard<'_, Connection> {
    p.db.lock().unwrap_or_else(|e| e.into_inner())
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn validate_email(email: &str) -> Result<(), AccountError> {
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AccountError::Invalid("Enter an email address."));
    };
    if local.is_empty()
        || domain.len() < 3
        || !domain.contains('.')
        || email.contains(char::is_whitespace)
        || email.len() > 254
    {
        return Err(AccountError::Invalid("Enter an email address."));
    }
    Ok(())
}

fn short_did(did: &str) -> String {
    let bare = did.trim_start_matches("did:sol:");
    if bare.len() <= 12 {
        return did.to_string();
    }
    format!("did:sol:{}…{}", &bare[..4], &bare[bare.len() - 4..])
}

fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unix_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn load_or_create_operator(dir: &Path) -> Result<Identity, String> {
    let path = dir.join("operator.json");
    if path.exists() {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let ed = decode_seed(v.get("ed").and_then(|s| s.as_str()).unwrap_or(""))?;
        let x = decode_seed(v.get("x").and_then(|s| s.as_str()).unwrap_or(""))?;
        return Ok(Identity::from_seeds(ed, x));
    }
    if dir.join("describe.json").exists() {
        return Err("data dir has a knot snapshot but no operator.json".into());
    }
    let id = Identity::generate();
    let (ed, x) = id.to_seeds();
    let body = serde_json::json!({
        "ed": hex::encode(ed),
        "x": hex::encode(x),
    });
    write_private(
        &path,
        serde_json::to_vec_pretty(&body).map_err(|e| e.to_string())?,
    )?;
    Ok(id)
}

fn decode_seed(hex_s: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_s).map_err(|e| e.to_string())?;
    bytes
        .try_into()
        .map_err(|_| "operator seed must be 32 bytes".into())
}

fn write_private(path: &Path, bytes: Vec<u8>) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

fn restrict_file(path: &PathBuf) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}
