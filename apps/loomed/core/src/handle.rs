use crate::error::{LoomedError, Result};
use crate::types::Peer;
use tangled::{Did, KnotClient};

pub fn parse_peer(s: &str) -> Result<Peer> {
    let s = s.trim();
    if s.is_empty() {
        return Err(LoomedError::Handle("empty peer".into()));
    }
    if s.starts_with("did:sol:") {
        let did = Did::parse(s).map_err(|e| LoomedError::Handle(e.to_string()))?;
        Ok(Peer::Did(did))
    } else {
        Ok(Peer::Handle(s.to_string()))
    }
}

pub async fn resolve_handle(client: &KnotClient, handle: &str) -> Result<Did> {
    let resp = client
        .identity_resolve(Some(handle), None)
        .await
        .map_err(|e| LoomedError::Handle(e.to_string()))?;
    tangled::handle::parse_well_known(&resp.did).map_err(|e| LoomedError::Handle(e.to_string()))?;
    Did::parse(&resp.did).map_err(|e| LoomedError::Handle(e.to_string()))
}
