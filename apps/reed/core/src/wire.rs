use crate::error::{ReedError, Result};
use crate::identity::ReedKeys;
use crate::outbox::Outbox;
use crate::replica::Replica;

pub(crate) const POST: &str = "pub.tangled.post";
pub(crate) const MEDIA: &str = "pub.tangled.media";
pub(crate) const FOLLOW: &str = "tangled.graph.follow";
pub(crate) const BLOCK: &str = "tangled.graph.block";
pub(crate) const LIKE: &str = "pub.reed.like";
pub(crate) const BOOST: &str = "pub.reed.boost";
pub(crate) const PROFILE: &str = "pub.reed.profile";
pub(crate) const MUTE: &str = "pub.reed.mute";
pub(crate) const FOREIGN: &str = "sys.foreign";
pub(crate) const SCHEMA: &str = "sys.schema";
pub(crate) const MAX_MEDIA: usize = 4;
pub(crate) const MAX_TEXT: usize = 32 * 1024;
pub(crate) const DEFAULT_MAX_BLOB: u64 = 134_217_728;

pub(crate) fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

pub(crate) fn cid_safe(s: &str) -> String {
    s.replace([':', '/'], ".")
}

pub(crate) fn unsigned(
    author: &str,
    collection: &str,
    rkey: &str,
    vis: &str,
    body: serde_json::Value,
    env_ref: Option<&str>,
) -> serde_json::Value {
    let mut v = serde_json::json!({
        "v": 1,
        "type": tangled::Cid::dag_cbor(collection.as_bytes()).to_string(),
        "collection": collection,
        "rkey": rkey,
        "author": author,
        "ts": now_us(),
        "prev": [],
        "vis": vis,
        "body": body,
    });
    if let Some(r) = env_ref {
        v["ref"] = serde_json::Value::String(r.to_string());
    }
    v
}

pub(crate) fn sign(keys: &ReedKeys, unsigned: &serde_json::Value) -> Result<serde_json::Value> {
    let signed = tangled::record::sign_json_record(keys.identity().signing_key(), unsigned)?;
    Ok(signed.json)
}

pub(crate) fn record_cid(json: &serde_json::Value) -> Result<String> {
    let mut unsigned = json.clone();
    let sig_hex = unsigned
        .as_object_mut()
        .and_then(|o| o.remove("sig"))
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| ReedError::Codec("missing sig".into()))?;
    let bytes = hex::decode(sig_hex).map_err(|e| ReedError::Codec(e.to_string()))?;
    let sig: [u8; 64] = bytes
        .try_into()
        .map_err(|_| ReedError::Codec("sig must be 64 bytes".into()))?;
    Ok(tangled::record::signed_record_cid(&unsigned, &sig)?.to_string())
}

pub(crate) fn put_signed(
    replica: &mut Replica,
    outbox: &mut Outbox,
    rec: serde_json::Value,
) -> Result<String> {
    let cid = record_cid(&rec)?;
    replica
        .apply_record(&serde_json::to_vec(&rec).map_err(|e| ReedError::Codec(e.to_string()))?)?;
    outbox.enqueue_record(rec)?;
    Ok(cid)
}

pub(crate) fn ensure_schema(
    keys: &ReedKeys,
    replica: &mut Replica,
    outbox: &mut Outbox,
    name: &str,
    fields: serde_json::Value,
) -> Result<()> {
    if replica.has_schema(name) {
        return Ok(());
    }
    let author = keys.did().as_str();
    let rkey = tangled::identity::tid_now();
    let body = serde_json::json!({
        "v": 1,
        "name": name,
        "fields": fields,
    });
    let rec = sign(
        keys,
        &unsigned(&author, SCHEMA, &rkey, "public", body, None),
    )?;
    put_signed(replica, outbox, rec)?;
    Ok(())
}
