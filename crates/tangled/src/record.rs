//! Records: unsigned/signed maps, visibility, ciphertext, commit objects.

use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;

use crate::cid::Cid;
use crate::dagcbor::{self, Value};
use crate::error::{Error, Result};
use crate::identity::valid_rkey;
use crate::sign;

/// Record visibility class (spec 08).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Public,
    Cap,
    Sealed,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Cap => "cap",
            Self::Sealed => "sealed",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "public" => Ok(Self::Public),
            "cap" => Ok(Self::Cap),
            "sealed" => Ok(Self::Sealed),
            other => Err(Error::Record(format!("unknown vis {other}"))),
        }
    }
}

/// Sign a JSON unsigned record (CID fields may be strings, matching vectors).
pub fn sign_json_record(sk: &SigningKey, unsigned: &serde_json::Value) -> Result<SignedRecord> {
    let cbor = dagcbor::encode_json(unsigned)?;
    let sig = sign::sign_unsigned(sk, &cbor);
    let mut signed = unsigned.clone();
    let obj = signed
        .as_object_mut()
        .ok_or_else(|| Error::Record("unsigned record must be a map".into()))?;
    obj.insert("sig".into(), serde_json::Value::String(hex::encode(sig)));
    // CID of complete DAG-CBOR including sig as bytes, not hex string.
    let mut value = dagcbor::from_json(unsigned)?;
    match &mut value {
        Value::Map(m) => {
            m.insert("sig".into(), Value::Bytes(sig.to_vec()));
        }
        _ => return Err(Error::Record("unsigned record must be a map".into())),
    }
    let signed_cbor = dagcbor::encode(&value)?;
    let cid = Cid::dag_cbor(&signed_cbor);
    Ok(SignedRecord {
        unsigned_cbor: cbor,
        signed_cbor,
        sig,
        cid,
        json: signed,
    })
}

/// Result of signing a record.
#[derive(Clone, Debug)]
pub struct SignedRecord {
    pub unsigned_cbor: Vec<u8>,
    pub signed_cbor: Vec<u8>,
    pub sig: [u8; 64],
    pub cid: Cid,
    /// JSON copy with `sig` as hex (for display); hashing uses `signed_cbor`.
    pub json: serde_json::Value,
}

/// Verify a signature over unsigned DAG-CBOR.
pub fn verify_json_record(
    pk: &[u8; 32],
    unsigned: &serde_json::Value,
    sig: &[u8; 64],
) -> Result<()> {
    let cbor = dagcbor::encode_json(unsigned)?;
    sign::verify_unsigned_raw(pk, &cbor, sig)
}

/// CID of the **complete** signed record (sig as 64 bytes).
pub fn signed_record_cid(unsigned: &serde_json::Value, sig: &[u8; 64]) -> Result<Cid> {
    let mut value = dagcbor::from_json(unsigned)?;
    match &mut value {
        Value::Map(m) => {
            m.insert("sig".into(), Value::Bytes(sig.to_vec()));
        }
        _ => return Err(Error::Record("unsigned record must be a map".into())),
    }
    Ok(Cid::dag_cbor(&dagcbor::encode(&value)?))
}

/// Drop `body` and `sig` from an unsigned/signed map (HPKE AAD).
pub fn unsigned_without_body_and_sig(unsigned: &serde_json::Value) -> Result<Vec<u8>> {
    let mut v = unsigned.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.remove("body");
        obj.remove("sig");
    }
    dagcbor::encode_json(&v)
}

/// Validate envelope rules that do not need a schema.
pub fn validate_envelope(unsigned: &serde_json::Value) -> Result<()> {
    let obj = unsigned
        .as_object()
        .ok_or_else(|| Error::Record("record must be a map".into()))?;
    let v = obj
        .get("v")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| Error::Record("missing v".into()))?;
    if v != 1 {
        return Err(Error::Record("v must be 1".into()));
    }
    let collection = obj
        .get("collection")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Record("missing collection".into()))?;
    let rkey = obj
        .get("rkey")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Record("missing rkey".into()))?;
    if !valid_rkey(rkey) {
        return Err(Error::Record(format!("invalid rkey {rkey}")));
    }
    let vis = obj
        .get("vis")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Record("missing vis".into()))?;
    Visibility::parse(vis)?;
    validate_signer_fields(unsigned)?;
    if vis == "public" {
        annotation_target(unsigned)?;
    }
    if let Some(circle) = obj.get("circle") {
        if vis != "sealed" {
            return Err(Error::Record(
                "circle-scoped records MUST use vis sealed".into(),
            ));
        }
        let c = circle
            .as_str()
            .ok_or_else(|| Error::Record("circle must be text".into()))?;
        if !c.contains('#') {
            return Err(Error::Record(
                "circle MUST be did:sol:<owner>#<circle-rkey>".into(),
            ));
        }
    }
    let _ = collection;
    Ok(())
}

/// `signedBy` + `proof` are both required for device signatures and both absent for identity.
pub fn validate_signer_fields(unsigned: &serde_json::Value) -> Result<()> {
    let has_sb = unsigned.get("signedBy").is_some();
    let has_proof = unsigned
        .get("proof")
        .and_then(|p| p.as_array())
        .is_some_and(|a| !a.is_empty());
    match (has_sb, has_proof) {
        (true, true) | (false, false) => Ok(()),
        _ => Err(Error::Record(
            "signedBy and proof are REQUIRED together for device keys and MUST be absent when the identity key signs".into(),
        )),
    }
}

/// Public-key that must verify `sig`: `signedBy` when present, else `author`.
pub fn signer_pubkey(unsigned: &serde_json::Value) -> Result<[u8; 32]> {
    if let Some(sb) = unsigned.get("signedBy") {
        match sb {
            serde_json::Value::String(s) => {
                if let Ok(b) = hex::decode(s) {
                    if let Ok(pk) = <[u8; 32]>::try_from(b.as_slice()) {
                        return Ok(pk);
                    }
                }
                crate::identity::decode_pubkey_base58(s)
            }
            _ => Err(Error::Record(
                "signedBy must be 32-byte key hex or base58".into(),
            )),
        }
    } else {
        let author = unsigned
            .get("author")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Record("missing author".into()))?;
        Ok(crate::identity::Did::parse(author)?.pubkey)
    }
}

/// Envelope `ref` is authoritative for thread assembly (spec 06).
pub fn annotation_target(unsigned: &serde_json::Value) -> Result<Option<String>> {
    let vis = unsigned
        .get("vis")
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    let env_ref = unsigned.get("ref").and_then(|v| v.as_str());
    let reply = unsigned
        .get("body")
        .and_then(|b| b.get("reply"))
        .and_then(|v| v.as_str());
    match (env_ref, reply) {
        (Some(r), Some(p)) if r == p => Ok(Some(r.to_string())),
        (Some(_), Some(_)) if vis == "public" => Err(Error::Record(
            "public record reply disagrees with envelope ref".into(),
        )),
        (Some(r), Some(_)) => Ok(Some(r.to_string())),
        (Some(r), None) => Ok(Some(r.to_string())),
        (None, Some(p)) => Ok(Some(p.to_string())),
        (None, None) => Ok(None),
    }
}

/// Collection merge class (spec 06). Only `pub.tangled.page` is a CRDT / compactable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MergeClass {
    Crdt,
    LastTsWins,
}

pub fn merge_class(collection: &str) -> MergeClass {
    if collection == "pub.tangled.page" {
        MergeClass::Crdt
    } else {
        MergeClass::LastTsWins
    }
}

pub fn compact_allowed(collection: &str) -> bool {
    merge_class(collection) == MergeClass::Crdt
}

/// Build a commit map (unsigned).
pub fn unsigned_commit(did: &str, data: &Cid, rev: &str, prev: Option<&Cid>) -> serde_json::Value {
    serde_json::json!({
        "v": 1,
        "did": did,
        "data": data.to_string_multibase(),
        "rev": rev,
        "prev": prev.map(|c| serde_json::Value::String(c.to_string_multibase()))
            .unwrap_or(serde_json::Value::Null),
    })
}

/// Sign a commit (omits `sig` and `sigRotation` from the unsigned map).
pub fn sign_commit(sk: &SigningKey, unsigned: &serde_json::Value) -> Result<([u8; 64], Vec<u8>)> {
    let cbor = dagcbor::encode_json(unsigned)?;
    Ok((sign::sign_unsigned(sk, &cbor), cbor))
}

/// Signed commit block: DAG-CBOR including `sig` as 64 bytes.
#[derive(Clone, Debug)]
pub struct SignedCommit {
    pub unsigned: serde_json::Value,
    pub sig: [u8; 64],
    pub signed_cbor: Vec<u8>,
    pub cid: Cid,
}

/// Sign and CID a commit (same off-chain envelope as records).
pub fn seal_commit(sk: &SigningKey, unsigned: &serde_json::Value) -> Result<SignedCommit> {
    let (sig, _) = sign_commit(sk, unsigned)?;
    let mut value = dagcbor::from_json(unsigned)?;
    match &mut value {
        Value::Map(m) => {
            m.insert("sig".into(), Value::Bytes(sig.to_vec()));
        }
        _ => return Err(Error::Record("unsigned commit must be a map".into())),
    }
    let signed_cbor = dagcbor::encode(&value)?;
    Ok(SignedCommit {
        unsigned: unsigned.clone(),
        sig,
        signed_cbor: signed_cbor.clone(),
        cid: Cid::dag_cbor(&signed_cbor),
    })
}

/// Verify a commit signature over the unsigned map.
pub fn verify_commit(pk: &[u8; 32], unsigned: &serde_json::Value, sig: &[u8; 64]) -> Result<()> {
    let cbor = dagcbor::encode_json(unsigned)?;
    sign::verify_unsigned_raw(pk, &cbor, sig)
}

/// Fields extracted from a signed commit block.
#[derive(Clone, Debug)]
pub struct CommitInfo {
    pub did: String,
    pub data: String,
    pub rev: String,
    pub prev: Option<String>,
    pub sig: [u8; 64],
    pub unsigned: serde_json::Value,
}

/// True when a DAG-CBOR / JSON map looks like a Commit (not a record).
pub fn is_commit_map(v: &serde_json::Value) -> bool {
    v.get("collection").is_none()
        && v.get("did").and_then(|x| x.as_str()).is_some()
        && v.get("data").is_some()
        && v.get("rev").and_then(|x| x.as_str()).is_some()
}

fn cid_field(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(m) => m.get("/").and_then(|x| x.as_str()).map(str::to_string),
        _ => None,
    }
}

fn sig_field(v: &serde_json::Value) -> Option<[u8; 64]> {
    match v {
        serde_json::Value::String(s) => {
            let b = hex::decode(s).ok()?;
            <[u8; 64]>::try_from(b.as_slice()).ok()
        }
        serde_json::Value::Object(m) => {
            let inner = m.get("/")?.as_object()?;
            let b64 = inner.get("bytes")?.as_str()?;
            use base64::Engine;
            let b = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            <[u8; 64]>::try_from(b.as_slice()).ok()
        }
        _ => None,
    }
}

/// Parse a signed commit from DAG-CBOR bytes or a JSON mapping of that block.
pub fn parse_commit(cbor_or_json: &[u8]) -> Result<CommitInfo> {
    let json = if let Ok(v) = dagcbor::decode(cbor_or_json) {
        dagcbor::to_json(&v)
    } else {
        serde_json::from_slice(cbor_or_json)
            .map_err(|e| Error::Record(format!("commit json: {e}")))?
    };
    parse_commit_json(&json)
}

pub fn parse_commit_json(json: &serde_json::Value) -> Result<CommitInfo> {
    if !is_commit_map(json) {
        return Err(Error::Record("not a commit map".into()));
    }
    let did = json
        .get("did")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Record("commit missing did".into()))?
        .to_string();
    let data = json
        .get("data")
        .and_then(cid_field)
        .ok_or_else(|| Error::Record("commit missing data".into()))?;
    let rev = json
        .get("rev")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Record("commit missing rev".into()))?
        .to_string();
    let prev = json
        .get("prev")
        .and_then(|p| if p.is_null() { None } else { cid_field(p) });
    let sig = json
        .get("sig")
        .and_then(sig_field)
        .ok_or_else(|| Error::Record("commit missing sig".into()))?;
    let mut unsigned = json.clone();
    if let Some(o) = unsigned.as_object_mut() {
        o.remove("sig");
        o.remove("sigRotation");
    }
    Ok(CommitInfo {
        did,
        data,
        rev,
        prev,
        sig,
        unsigned,
    })
}

/// Last-ts-wins among causal heads (spec 06).
pub fn last_ts_wins<T: CausalRecord>(records: &[T]) -> Option<&T> {
    let heads = causal_heads(records);
    heads.into_iter().max_by(|a, b| {
        a.ts()
            .cmp(&b.ts())
            .then_with(|| a.cid_text().cmp(b.cid_text()))
    })
}

/// A same-key record for merge.
pub trait CausalRecord {
    fn cid_text(&self) -> &str;
    fn ts(&self) -> u64;
    fn prev(&self) -> &[String];
    fn is_tombstone(&self) -> bool;
}

/// Causal heads: records not superseded via `prev`.
pub fn causal_heads<T: CausalRecord>(records: &[T]) -> Vec<&T> {
    let mut superseded = std::collections::HashSet::new();
    let by_cid: std::collections::HashMap<&str, &T> =
        records.iter().map(|r| (r.cid_text(), r)).collect();
    fn walk<T: CausalRecord>(
        cid: &str,
        by_cid: &std::collections::HashMap<&str, &T>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        if !seen.insert(cid.to_string()) {
            return;
        }
        if let Some(r) = by_cid.get(cid) {
            for p in r.prev() {
                walk(p, by_cid, seen);
            }
        }
    }
    for r in records {
        let mut seen = std::collections::HashSet::new();
        for p in r.prev() {
            walk(p, &by_cid, &mut seen);
        }
        superseded.extend(seen);
    }
    records
        .iter()
        .filter(|r| !superseded.contains(r.cid_text()))
        .collect()
}

/// Helper to put a hex signature into a DAG-CBOR map as bytes.
pub fn map_with_sig_bytes(unsigned: &Value, sig: &[u8; 64]) -> Result<Value> {
    let mut m = match unsigned {
        Value::Map(m) => m.clone(),
        _ => return Err(Error::Record("expected map".into())),
    };
    m.insert("sig".into(), Value::Bytes(sig.to_vec()));
    Ok(Value::Map(m))
}

/// Build a ciphertext map (`alg = hpke-record` or `epoch-aead`).
pub fn ciphertext_hpke(enc: &[u8], ct: &[u8]) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    m.insert("alg".into(), Value::Text("hpke-record".into()));
    m.insert("enc".into(), Value::Bytes(enc.to_vec()));
    m.insert("ct".into(), Value::Bytes(ct.to_vec()));
    m
}

pub fn ciphertext_epoch(epoch: u64, ct: &[u8]) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    m.insert("alg".into(), Value::Text("epoch-aead".into()));
    m.insert("ct".into(), Value::Bytes(ct.to_vec()));
    m.insert("epoch".into(), Value::Unsigned(epoch));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_vis_rejected() {
        assert!(Visibility::parse("secret").is_err());
        assert_eq!(Visibility::parse("public").unwrap(), Visibility::Public);
        assert_eq!(Visibility::parse("cap").unwrap(), Visibility::Cap);
        assert_eq!(Visibility::parse("sealed").unwrap(), Visibility::Sealed);
    }

    #[test]
    fn last_ts_wins_must_not_compact() {
        assert!(!compact_allowed("pub.tangled.post"));
        assert!(!compact_allowed("pub.tangled.annotation"));
        assert!(!compact_allowed("tangled.graph.follow"));
        assert!(compact_allowed("pub.tangled.page"));
    }

    #[test]
    fn public_reply_ref_mismatch_rejected() {
        let rec = serde_json::json!({
            "v": 1,
            "collection": "pub.tangled.annotation",
            "rkey": "a1",
            "vis": "public",
            "ref": "bafyaaaa",
            "body": { "text": "hi", "reply": "bafybbbb" }
        });
        assert!(annotation_target(&rec).is_err());
    }

    #[test]
    fn ref_authoritative_when_agrees() {
        let rec = serde_json::json!({
            "vis": "public",
            "ref": "bafyaaaa",
            "body": { "reply": "bafyaaaa" }
        });
        assert_eq!(
            annotation_target(&rec).unwrap().as_deref(),
            Some("bafyaaaa")
        );
    }

    #[test]
    fn signed_by_requires_proof() {
        let rec = serde_json::json!({ "signedBy": "aa".repeat(32) });
        assert!(validate_signer_fields(&rec).is_err());
        let rec = serde_json::json!({
            "signedBy": "aa".repeat(32),
            "proof": ["bafyproof"]
        });
        assert!(validate_signer_fields(&rec).is_ok());
    }

    #[test]
    fn signed_commit_roundtrip_and_prev() {
        let id = crate::identity::Identity::from_seeds([9u8; 32], [10u8; 32]);
        let data = Cid::dag_cbor(b"mst-root");
        let genesis = unsigned_commit(&id.did_string(), &data, "0000000000001", None);
        let sealed = seal_commit(id.signing_key(), &genesis).unwrap();
        verify_commit(&id.pubkey_bytes(), &genesis, &sealed.sig).unwrap();
        let parsed = parse_commit(&sealed.signed_cbor).unwrap();
        assert_eq!(parsed.did, id.did_string());
        assert!(parsed.prev.is_none());
        assert_eq!(parsed.rev, "0000000000001");
        let second_data = Cid::dag_cbor(b"mst-root-2");
        let next = unsigned_commit(
            &id.did_string(),
            &second_data,
            "0000000000002",
            Some(&sealed.cid),
        );
        let sealed2 = seal_commit(id.signing_key(), &next).unwrap();
        let parsed2 = parse_commit(&sealed2.signed_cbor).unwrap();
        assert_eq!(
            parsed2.prev.as_deref(),
            Some(sealed.cid.to_string().as_str())
        );
        let mut bad = genesis.clone();
        bad["did"] = serde_json::json!("did:sol:nope");
        assert!(verify_commit(&id.pubkey_bytes(), &bad, &sealed.sig).is_err());
    }
}
