//! Owner-confirmed erasure across hosted workers, archive storage, and repository blocks.
use super::*;
#[derive(Deserialize)]
struct EraseRequest {
    confirmation: String,
    source: Option<String>,
    account: Option<String>,
}
pub(super) fn router() -> Router<ImportState> {
    Router::new().route("/api/imports/erase", post(erase))
}
async fn erase(
    State(state): State<ImportState>,
    headers: HeaderMap,
    Json(request): Json<EraseRequest>,
) -> Result<Json<tangled_import::store::erasure::ErasureReceipt>, HttpError> {
    same_origin(&headers)?;
    let (owner, id) = auth(&state, &headers)?;
    if request.confirmation != "ERASE" {
        return Err(bad(
            "Type ERASE to confirm permanent deletion from this host.",
        ));
    }
    if request.source.is_some() != request.account.is_some() {
        return Err(bad(
            "Choose both a source and account, or the entire archive.",
        ));
    }
    // The control service must remove credentials and stop its workers first.
    // If it cannot acknowledge removal, preserve local data for a safe retry.
    let _pairing = pairing::lifecycle_guard().await;
    pairing::stop_scope_locked(&owner,request.source.as_deref(),request.account.as_deref()).await
        .map_err(|e|failure(StatusCode::SERVICE_UNAVAILABLE,format!("Hosted source authorization could not be removed: {e}. No local erasure was completed; retry when the pairing service is reachable.")))?;
    tokio::task::spawn_blocking(move || {
        let mut store = state.archive.lock().map_err(bad)?;
        let mut engine = state.engine.lock().map_err(bad)?;
        if engine.signing_keys.get(&owner) != Some(&id.to_seeds().0) {
            return Err(failure(
                StatusCode::FORBIDDEN,
                "This host no longer signs for the account.",
            ));
        }
        let receipt = match (request.source, request.account) {
            (Some(source), Some(account)) => {
                store.erase_source(&owner, &id, &mut engine, &source, &account)
            }
            _ => store.erase_all(&owner, &id, &mut engine),
        }
        .map_err(bad)?;
        Ok(Json(receipt))
    })
    .await
    .map_err(|_| {
        failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erasure stopped before completion. Retry to finish removing retained data.",
        )
    })?
}
