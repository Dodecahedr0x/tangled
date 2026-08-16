use crate::error::{LoomedError, Result};
use crate::identity::LoomedKeys;
use crate::types::CircleRef;
use tangled::circle::CircleState;
use tangled::hpke;
use tangled::record;

pub(crate) fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

pub(crate) fn unsigned(
    author: &str,
    collection: &str,
    rkey: &str,
    vis: &str,
    body: serde_json::Value,
    circle: Option<&str>,
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
    if let Some(c) = circle {
        v["circle"] = serde_json::Value::String(c.to_string());
    }
    if let Some(r) = env_ref {
        v["ref"] = serde_json::Value::String(r.to_string());
    }
    v
}

pub(crate) fn sign(keys: &LoomedKeys, unsigned: &serde_json::Value) -> Result<serde_json::Value> {
    let id = keys.identity();
    let signed = record::sign_json_record(id.signing_key(), unsigned)?;
    Ok(signed.json)
}

pub(crate) fn seal_json(
    keys: &LoomedKeys,
    circle: &CircleRef,
    state: &CircleState,
    collection: &str,
    plaintext: serde_json::Value,
    env_ref: Option<&str>,
) -> Result<serde_json::Value> {
    let author = keys.did().as_str();
    let rkey = tangled::identity::tid_now();
    let mut unsigned = unsigned(
        &author,
        collection,
        &rkey,
        "sealed",
        serde_json::json!({}),
        Some(&circle.0),
        env_ref,
    );
    unsigned["epoch"] = serde_json::json!(state.epoch);
    let aad = record::unsigned_without_body_and_sig(&unsigned)?;
    let secret = state
        .epoch_secrets
        .get(&state.epoch)
        .ok_or_else(|| LoomedError::Codec("missing epoch secret".into()))?;
    let pt = serde_json::to_vec(&plaintext).map_err(|e| LoomedError::Codec(e.to_string()))?;
    let ct = hpke::epoch_aead_seal(secret, &circle.0, &aad, &pt)?;
    unsigned["body"] = serde_json::json!({
        "alg": "epoch-aead",
        "epoch": state.epoch,
        "ct": hex::encode(ct),
    });
    sign(keys, &unsigned)
}

pub(crate) fn open_plaintext(
    state: &CircleState,
    record: &serde_json::Value,
) -> Result<serde_json::Value> {
    let body = record
        .get("body")
        .ok_or_else(|| LoomedError::Codec("missing body".into()))?;
    let alg = body.get("alg").and_then(|a| a.as_str()).unwrap_or("");
    if alg != "epoch-aead" {
        return Err(LoomedError::Codec(format!("unsupported alg {alg}")));
    }
    let ct_hex = body
        .get("ct")
        .and_then(|c| c.as_str())
        .ok_or_else(|| LoomedError::Codec("missing ct".into()))?;
    let ct = hex::decode(ct_hex).map_err(|e| LoomedError::Codec(e.to_string()))?;
    let epoch = body
        .get("epoch")
        .and_then(|e| e.as_u64())
        .or_else(|| record.get("epoch").and_then(|e| e.as_u64()))
        .unwrap_or(state.epoch);
    let secret = state
        .epoch_secrets
        .get(&epoch)
        .ok_or_else(|| LoomedError::Codec("missing epoch secret".into()))?;
    let mut unsigned = record.clone();
    if let Some(obj) = unsigned.as_object_mut() {
        obj.remove("sig");
    }
    let aad = record::unsigned_without_body_and_sig(&unsigned)?;
    let circle = record
        .get("circle")
        .and_then(|c| c.as_str())
        .ok_or_else(|| LoomedError::Codec("missing circle".into()))?;
    let pt = hpke::epoch_aead_open(secret, circle, &aad, &ct)?;
    serde_json::from_slice(&pt).map_err(|e| LoomedError::Codec(e.to_string()))
}

pub(crate) fn signed_cid(record: &serde_json::Value) -> Result<String> {
    let sig_hex = record
        .get("sig")
        .and_then(|s| s.as_str())
        .ok_or_else(|| LoomedError::Codec("missing sig".into()))?;
    let sig = hex::decode(sig_hex).map_err(|e| LoomedError::Codec(e.to_string()))?;
    let sig: [u8; 64] = sig
        .try_into()
        .map_err(|_| LoomedError::Codec("sig must be 64 bytes".into()))?;
    let mut unsigned = record.clone();
    if let Some(obj) = unsigned.as_object_mut() {
        obj.remove("sig");
    }
    Ok(record::signed_record_cid(&unsigned, &sig)?.to_string())
}
