//! Owner-authorized proxy to an explicitly configured loopback native-auth broker.
use super::{auth, bad, failure, same_origin, HttpError, ImportState};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, sync::OnceLock, time::Duration};
use tokio::sync::Mutex;

// Serializes connection issuance with stop_scope. Never hold an archive/engine
// mutex across a broker request; erasure calls this before locking those stores.
static BINDING: OnceLock<Mutex<()>> = OnceLock::new();

fn control_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Invalid pairing control URL".to_owned())?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Pairing control requires an HTTP literal loopback address without credentials, path or query".into());
    }
    Ok(url)
}

struct BrokerClient {
    url: reqwest::Url,
    token: String,
    client: reqwest::Client,
}

impl BrokerClient {
    fn configured() -> Result<Option<Self>, String> {
        let (url, file) = (
            std::env::var("TANGLED_PAIRING_URL"),
            std::env::var("TANGLED_PAIRING_TOKEN_FILE"),
        );
        let (url, file) = match (url, file) {
            (Err(_), Err(_)) => return Ok(None),
            (Ok(url), Ok(file)) => (url, file),
            _ => return Err("The host pairing service needs both its loopback URL and private control token file".into()),
        };
        let url = control_url(&url)?;
        let metadata =
            fs::symlink_metadata(&file).map_err(|_| "Pairing control token file is unavailable")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("Pairing control token file must be private (0600)".into());
            }
        }
        if !metadata.file_type().is_file() || metadata.len() > 4096 {
            return Err("Pairing control token must be a bounded regular file".into());
        }
        let token = fs::read_to_string(file)
            .map_err(|_| "Pairing control token file cannot be read")?
            .trim()
            .to_owned();
        if token.len() < 32 || token.contains(['\r', '\n']) || !token.is_ascii() {
            return Err(
                "Pairing control token must contain at least 32 ASCII bytes on one line".into(),
            );
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(130))
            .build()
            .map_err(|_| "Cannot initialize pairing control client")?;
        Ok(Some(Self { url, token, client }))
    }

    async fn request<T: DeserializeOwned>(
        &self,
        operation: &str,
        payload: Option<Value>,
    ) -> Result<T, String> {
        let url = self
            .url
            .join(operation)
            .map_err(|_| "Invalid pairing control operation")?;
        let request = if let Some(payload) = payload {
            self.client.post(url).json(&payload)
        } else {
            self.client.get(url)
        };
        let mut response = request.bearer_auth(&self.token).send().await.map_err(|_| {
            "The host pairing service is unavailable. No completion has been acknowledged."
                .to_owned()
        })?;
        if !response.status().is_success() {
            return Err("The host pairing service rejected this operation. Verify the challenge and host configuration.".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Pairing service response failed")?
        {
            if bytes.len() + chunk.len() > 512 * 1024 {
                return Err("Pairing service response exceeds its limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "The pairing service returned an invalid response".into())
    }
}

fn unavailable(message: impl ToString) -> HttpError {
    failure(StatusCode::SERVICE_UNAVAILABLE, message)
}

fn broker() -> Result<BrokerClient, HttpError> {
    BrokerClient::configured()
        .map_err(unavailable)?
        .ok_or_else(|| {
            unavailable(
                "Browser pairing is unavailable until this host configures its pairing service.",
            )
        })
}

#[derive(Serialize, Deserialize)]
struct PairingStatus {
    id: String,
    source: String,
    status: String,
    expires_at: u64,
    #[serde(skip_serializing)]
    generation: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qr_svg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authorize_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn authorized(
    state: &ImportState,
    headers: &HeaderMap,
) -> Result<(String, tangled::Identity), HttpError> {
    let (owner, identity) = auth(state, headers)?;
    let engine = state.engine.lock().map_err(bad)?;
    if engine.signing_keys.get(&owner) != Some(&identity.to_seeds().0) {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "This host no longer signs for this account.",
        ));
    }
    Ok((owner, identity))
}

async fn bind_if_authorized(
    state: &ImportState,
    headers: &HeaderMap,
    broker: &BrokerClient,
    mut result: PairingStatus,
) -> Result<Json<PairingStatus>, HttpError> {
    // Provider URLs are fixed rather than arbitrary redirects returned to a browser.
    if let Some(url) = &result.authorize_url {
        let url = reqwest::Url::parse(url).map_err(bad)?;
        if url.scheme() != "https"
            || url.host_str() != Some("x.com")
            || url.path() != "/i/oauth2/authorize"
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(unavailable(
                "The pairing service returned an invalid provider URL.",
            ));
        }
    }
    if result.status != "authorized" {
        return Ok(Json(result));
    }
    let (owner, identity) = authorized(state, headers)?;
    // Recheck after serialization so simultaneous browser polls issue one connection.
    result = broker
        .request("status", Some(json!({"owner":owner,"id":result.id})))
        .await
        .map_err(unavailable)?;
    if result.status != "authorized" {
        return Ok(Json(result));
    }
    let account = result
        .account
        .as_deref()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            unavailable("The source did not return its authenticated account identity.")
        })?;
    let connected = {
        let mut archive = state.archive.lock().map_err(bad)?;
        // Recheck custody under the same engine lock used to fence migrations.
        let engine = state.engine.lock().map_err(bad)?;
        if engine.signing_keys.get(&owner) != Some(&identity.to_seeds().0) {
            return Err(failure(
                StatusCode::FORBIDDEN,
                "This host no longer signs for this account.",
            ));
        }
        archive
            .check_erasure_generation(&owner, result.generation)
            .map_err(bad)?;
        archive.remember_signer(&owner, &identity).map_err(bad)?;
        // Revoke old native-account capabilities on reconnect before binding the replacement.
        for previous in archive.connections(&owner).map_err(bad)? {
            if previous.source == result.source && previous.account == account {
                archive.disconnect(&owner, &previous.id).map_err(bad)?;
            }
        }
        archive
            .connect(&owner, &result.source, account)
            .map_err(bad)?
    };
    let binding = json!({"owner":owner,"id":result.id,"binding":{"connection_id":connected.connection.id,"token":connected.token}});
    match broker.request("bind", Some(binding)).await {
        Ok(result) => Ok(Json(result)),
        Err(error) => {
            // Even an ambiguous timeout cannot leave a live ingestion capability.
            state
                .archive
                .lock()
                .map_err(bad)?
                .disconnect(&owner, &connected.connection.id)
                .map_err(bad)?;
            Err(unavailable(error))
        }
    }
}

#[derive(Deserialize)]
struct Start {
    source: String,
}
#[derive(Deserialize)]
struct Step {
    phone: Option<String>,
    code: Option<String>,
    password: Option<String>,
}
#[derive(Deserialize)]
struct Disconnect {
    id: String,
}
#[derive(Deserialize)]
struct Callback {
    state: String,
    code: Option<String>,
}

pub(super) fn router() -> Router<ImportState> {
    Router::new()
        .route("/api/imports/pairing", get(availability))
        .route("/api/imports/pairing/start", post(start))
        .route("/api/imports/pairing/disconnect", post(disconnect))
        .route("/api/imports/pairing/callback/x", get(callback))
        .route("/api/imports/pairing/{id}", get(status))
        .route("/api/imports/pairing/{id}/step", post(step))
}

async fn availability(
    State(state): State<ImportState>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    authorized(&state, &headers)?;
    let result = match BrokerClient::configured() {
        Ok(Some(client)) => client.request::<Value>("availability", None).await,
        Ok(None) => Err("This host has not configured browser pairing.".into()),
        Err(error) => Err(error),
    };
    Ok(Json(result.unwrap_or_else(
        |reason| json!({"available":false,"sources":[],"reason":reason}),
    )))
}

async fn start(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Json(payload): Json<Start>,
) -> Result<Json<PairingStatus>, HttpError> {
    let _lifecycle = lifecycle_guard().await;
    same_origin(&headers)?;
    let (owner, _) = authorized(&state, &headers)?;
    if !["telegram", "twitter", "whatsapp"].contains(&payload.source.as_str()) {
        return Err(bad("Unsupported source"));
    }
    let broker = broker()?;
    let generation = state
        .archive
        .lock()
        .map_err(bad)?
        .erasure_generation(&owner)
        .map_err(bad)?;
    let result = broker
        .request(
            "start",
            Some(json!({"owner":owner,"source":payload.source,"generation":generation})),
        )
        .await
        .map_err(unavailable)?;
    bind_if_authorized(&state, &headers, &broker, result).await
}

async fn status(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<PairingStatus>, HttpError> {
    let _lifecycle = lifecycle_guard().await;
    same_origin(&headers)?;
    let (owner, _) = authorized(&state, &headers)?;
    let broker = broker()?;
    let result = broker
        .request("status", Some(json!({"owner":owner,"id":id})))
        .await
        .map_err(unavailable)?;
    bind_if_authorized(&state, &headers, &broker, result).await
}

async fn step(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<Step>,
) -> Result<Json<PairingStatus>, HttpError> {
    let _lifecycle = lifecycle_guard().await;
    same_origin(&headers)?;
    let (owner, _) = authorized(&state, &headers)?;
    let mut values = serde_json::Map::new();
    for (key, value) in [
        ("phone", payload.phone),
        ("code", payload.code),
        ("password", payload.password),
    ] {
        if let Some(value) = value {
            if value.len() > 1024 {
                return Err(bad("Pairing input exceeds its limit"));
            }
            values.insert(key.into(), Value::String(value));
        }
    }
    let broker = broker()?;
    let result = broker
        .request("step", Some(json!({"owner":owner,"id":id,"values":values})))
        .await
        .map_err(unavailable)?;
    bind_if_authorized(&state, &headers, &broker, result).await
}

async fn callback(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Query(payload): Query<Callback>,
) -> Result<Redirect, HttpError> {
    let _lifecycle = lifecycle_guard().await;
    // OAuth is a deliberate external navigation. The broker requires the logged-in
    // owner's random, expiring, single-use state and its matching PKCE verifier.
    let (owner, _) = authorized(&state, &headers)?;
    if payload.state.len() > 256 || payload.code.as_ref().is_some_and(|code| code.len() > 4096) {
        return Err(bad("Invalid OAuth callback"));
    }
    let broker = broker()?;
    let result = broker.request("callback", Some(json!({"owner":owner,"state":payload.state,"code":payload.code.unwrap_or_default()}))).await.map_err(unavailable)?;
    let _ = bind_if_authorized(&state, &headers, &broker, result).await?;
    // Provider authorization codes are never retained in the application's URL.
    Ok(Redirect::to("/imports"))
}

async fn disconnect(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Json(payload): Json<Disconnect>,
) -> Result<Json<Value>, HttpError> {
    let _lifecycle = lifecycle_guard().await;
    same_origin(&headers)?;
    let (owner, _) = authorized(&state, &headers)?;
    let broker = broker()?;
    let result: Value = broker
        .request("disconnect", Some(json!({"owner":owner,"id":payload.id})))
        .await
        .map_err(unavailable)?;
    if let Some(connection) = result.get("connection_id").and_then(Value::as_str) {
        state
            .archive
            .lock()
            .map_err(bad)?
            .disconnect(&owner, connection)
            .map_err(bad)?;
    }
    Ok(Json(json!({"stopped":true})))
}

/// Must finish before archive erasure/migration takes its store locks. A configured
/// but unreachable service never acknowledges source credential erasure.
pub(super) async fn lifecycle_guard() -> tokio::sync::MutexGuard<'static, ()> {
    BINDING.get_or_init(|| Mutex::new(())).lock().await
}

pub(super) async fn stop_scope(
    owner: &str,
    source: Option<&str>,
    account: Option<&str>,
) -> Result<(), String> {
    let _lifecycle = lifecycle_guard().await;
    stop_scope_locked(owner, source, account).await
}

pub(super) async fn stop_scope_locked(
    owner: &str,
    source: Option<&str>,
    account: Option<&str>,
) -> Result<(), String> {
    if let Some(broker) = BrokerClient::configured()? {
        let result: Value = broker
            .request(
                "stop_scope",
                Some(json!({"owner":owner,"source":source,"account":account})),
            )
            .await?;
        if result.get("stopped") != Some(&Value::Bool(true)) {
            return Err("The pairing service did not acknowledge stopped workers and erased source credentials".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_endpoint_must_be_loopback_without_credentials_or_query() {
        assert!(control_url("http://127.0.0.1:8791").is_ok());
        assert!(control_url("http://[::1]:8791").is_ok());
        for url in [
            "https://example.com",
            "http://localhost:8791",
            "http://a:b@127.0.0.1:8791",
            "http://127.0.0.1:8791?token=secret",
            "file:///tmp/a",
        ] {
            assert!(control_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn public_pairing_drops_private_provider_and_connection_secrets() {
        let result: PairingStatus = serde_json::from_value(serde_json::json!({
            "id":"pairing", "source":"telegram", "status":"authorized", "expires_at":123,
            "account":"100", "generation":0, "token":"ingest-secret", "session":"provider-secret", "owner":"alice"
        })).unwrap();
        let body = serde_json::to_string(&result).unwrap();
        assert!(!body.contains("ingest-secret"));
        assert!(!body.contains("provider-secret"));
        assert!(!body.contains("alice"));
        assert!(body.contains("authorized"));
    }
}
