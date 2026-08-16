use crate::error::{ReedError, Result};
use crate::types::Peer;
use tangled::{Did, KnotClient};

pub fn parse_peer(s: &str) -> Result<Peer> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ReedError::Handle("empty peer".into()));
    }
    if s.starts_with("did:sol:") {
        let did = Did::parse(s).map_err(|e| ReedError::Handle(e.to_string()))?;
        return Ok(Peer::Did(did));
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Ok(Peer::Handle(s.to_string()));
    }
    if let Some(acct) = normalize_acct(s) {
        return Ok(Peer::Handle(acct));
    }
    Ok(Peer::Handle(s.to_string()))
}

/// `acct:user@host`, `@user@host`, or `user@host` → `acct:user@host`.
pub fn normalize_acct(s: &str) -> Option<String> {
    let s = s.trim();
    let rest = s
        .strip_prefix("acct:")
        .or_else(|| s.strip_prefix('@'))
        .unwrap_or(s);
    let (user, host) = rest.split_once('@')?;
    if user.is_empty() || host.is_empty() || !host.contains('.') {
        return None;
    }
    if user.contains('@') {
        return None;
    }
    Some(format!("acct:{user}@{host}"))
}

pub fn is_ap_resource(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("acct:")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || normalize_acct(s).is_some()
}

pub async fn resolve_handle(client: &KnotClient, handle: &str) -> Result<Did> {
    if is_ap_resource(handle) {
        return resolve_ap(client, handle).await;
    }
    let resp = client
        .identity_resolve(Some(handle), None)
        .await
        .map_err(|e| ReedError::Handle(e.to_string()))?;
    tangled::handle::parse_well_known(&resp.did).map_err(|e| ReedError::Handle(e.to_string()))?;
    Did::parse(&resp.did).map_err(|e| ReedError::Handle(e.to_string()))
}

pub async fn resolve_peer(client: &KnotClient, peer: &Peer) -> Result<Did> {
    match peer {
        Peer::Did(d) => Ok(d.clone()),
        Peer::Handle(h) => resolve_handle(client, h).await,
    }
}

async fn resolve_ap(client: &KnotClient, resource: &str) -> Result<Did> {
    let resource = normalize_acct(resource).unwrap_or_else(|| resource.to_string());
    let resp = client
        .ap_resolve(&resource)
        .await
        .map_err(|e| ReedError::Handle(e.to_string()))?;
    Did::parse(&resp.did).map_err(|e| ReedError::Handle(e.to_string()))
}
