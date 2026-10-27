//! Authenticated chunked uploads and idempotent import completion.
use super::*;
use axum::{body::Bytes, routing::put};
use tangled_import::store::uploads::{UploadRequest, UploadStatus, UPLOAD_CHUNK};

pub(super) fn router() -> Router<ImportState> {
    Router::new()
        .route("/api/imports/uploads", post(create))
        .route("/api/imports/uploads/{id}", get(status))
        .route("/api/imports/uploads/{id}/chunks", put(chunk))
        .route("/api/imports/uploads/{id}/complete", post(complete))
        .route("/api/imports/uploads/{id}/cancel", post(cancel))
}
fn check_signer(state: &ImportState, owner: &str, id: &Identity) -> Result<(), HttpError> {
    let engine = state.engine.lock().map_err(bad)?;
    if engine.signing_keys.get(owner) != Some(&id.to_seeds().0) {
        return Err(failure(
            StatusCode::FORBIDDEN,
            "This host no longer signs for the account.",
        ));
    }
    Ok(())
}
async fn create(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Json(request): Json<UploadRequest>,
) -> Result<Json<UploadStatus>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    if request.sha256.is_none() {
        return Err(bad(
            "Provide the SHA-256 file digest to make resuming safe.",
        ));
    }
    let mut store = state.archive.lock().map_err(bad)?;
    check_signer(&state, &owner, &id)?;
    Ok(Json(store.create_upload(&owner, request).map_err(bad)?))
}
async fn status(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(upload): Path<String>,
) -> Result<Json<UploadStatus>, HttpError> {
    let (owner, _) = auth(&state, &headers)?;
    Ok(Json(
        state
            .archive
            .lock()
            .map_err(bad)?
            .upload_status(&owner, &upload)
            .map_err(|e| failure(StatusCode::NOT_FOUND, e))?,
    ))
}
#[derive(Deserialize)]
struct ChunkQuery {
    offset: usize,
}
async fn chunk(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(upload): Path<String>,
    Query(query): Query<ChunkQuery>,
    bytes: Bytes,
) -> Result<Json<UploadStatus>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    if bytes.len() > UPLOAD_CHUNK {
        return Err(failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Chunks must be at most 1 MiB.",
        ));
    }
    let digest = headers
        .get("x-chunk-sha256")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| bad("Missing chunk SHA-256 digest"))?;
    use sha2::{Digest, Sha256};
    if hex::encode(Sha256::digest(&bytes)) != digest {
        return Err(bad("Chunk checksum mismatch. Retry the original file."));
    }
    let mut store = state.archive.lock().map_err(bad)?;
    check_signer(&state, &owner, &id)?;
    Ok(Json(
        store
            .append_upload(&owner, &upload, query.offset, &bytes)
            .map_err(|e| failure(StatusCode::CONFLICT, e))?,
    ))
}
async fn complete(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(upload): Path<String>,
) -> Result<Json<UploadStatus>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    // Parse outside the shared store/engine locks, then recheck upload/custody
    // under both locks before committing. Erasure/removal during parsing wins.
    tokio::task::spawn_blocking(move || {
        let (status, bytes) = {
            let store = state.archive.lock().map_err(bad)?;
            let status = store.upload_status(&owner, &upload).map_err(bad)?;
            if status.status == "complete" {
                return Ok(Json(status));
            }
            store.upload_bytes(&owner, &upload).map_err(bad)?
        };
        let batch =
            tangled_import::parse(&status.source, &status.account, &status.filename, &bytes)
                .map_err(bad)?;
        let mut store = state.archive.lock().map_err(bad)?;
        let current = store.upload_status(&owner, &upload).map_err(bad)?;
        if current.status == "complete" {
            return Ok(Json(current));
        }
        if current.status != "ready" {
            return Err(failure(
                StatusCode::CONFLICT,
                "Upload was canceled or erased.",
            ));
        }
        let mut engine = state.engine.lock().map_err(bad)?;
        if engine.signing_keys.get(&owner) != Some(&id.to_seeds().0) {
            return Err(failure(
                StatusCode::FORBIDDEN,
                "This host no longer signs for the account.",
            ));
        }
        store.remember_signer(&owner, &id).map_err(bad)?;
        let receipt = store
            .ingest(&owner, &id, &mut engine, &status.filename, &bytes, &batch)
            .map_err(bad)?;
        Ok(Json(
            store
                .finish_upload(&owner, &upload, &receipt)
                .map_err(bad)?,
        ))
    })
    .await
    .map_err(|_| {
        failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Import worker stopped. Resume the upload to retry.",
        )
    })?
}
async fn cancel(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Path(upload): Path<String>,
) -> Result<Json<serde_json::Value>, HttpError> {
    same_origin(&headers)?;
    let (owner, _) = auth(&state, &headers)?;
    state
        .archive
        .lock()
        .map_err(bad)?
        .cancel_upload(&owner, &upload)
        .map_err(bad)?;
    Ok(Json(json!({"canceled":true})))
}
