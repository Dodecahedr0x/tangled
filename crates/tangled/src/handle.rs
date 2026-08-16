//! DNS / well-known / SNS handle binding (spec 04 / 13). Bidirectional: the
//! proof names the DID and `sys/id.alsoKnownAs` lists the handle.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::identity::{decode_pubkey_base58, encode_pubkey_base58, Cluster, Did};

/// Bonfida SPL Name Service program.
pub const SNS_NAME_PROGRAM: &str = "namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX";
/// `.sol` TLD parent account.
pub const SNS_ROOT_DOMAIN: &str = "58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx";
const SNS_HASH_PREFIX: &[u8] = b"SPL Name Service";
const SNS_HEADER_LEN: usize = 96;

/// TXT / well-known / SNS lookup. Tests inject maps; production sets `live`
/// and `solana_rpc` (`SOLANA_RPC`).
#[derive(Clone, Debug, Default)]
pub struct HandleResolver {
    /// `_tangled.<host>` → TXT strings.
    pub txt: HashMap<String, Vec<String>>,
    /// well-known URL → body.
    pub well_known: HashMap<String, String>,
    /// Injected SNS domain (`alice.sol`) → registry. Tests use this; no RPC.
    pub sns: HashMap<String, SnsRecord>,
    /// When true, misses fall through to DNS (DoH) and HTTPS. Tests leave this false.
    pub live: bool,
    /// JSON-RPC URL for live SNS (`getAccountInfo`). Tests leave this `None`.
    pub solana_rpc: Option<String>,
}

/// Parsed Solana Name Service registry (spec 13).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnsRecord {
    pub owner: [u8; 32],
    pub tangled: Option<String>,
    pub head: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleProof {
    pub did: String,
    pub head: Option<String>,
}

pub fn txt_name(handle: &str) -> String {
    format!("_tangled.{handle}")
}

pub fn well_known_url(handle: &str) -> String {
    format!("https://{handle}/.well-known/tangled-did")
}

/// Extract a DID from TXT values (`did:sol:…` or `head=…` ignored).
pub fn did_from_txt(txt_values: &[String]) -> Option<String> {
    txt_values
        .iter()
        .find(|v| v.starts_with("did:sol:"))
        .cloned()
}

pub fn head_from_txt(txt_values: &[String]) -> Option<String> {
    txt_values.iter().find_map(|v| {
        v.strip_prefix("head=")
            .or_else(|| v.strip_prefix("head = "))
            .map(str::to_string)
    })
}

/// Bidirectional handle check. One-sided claims MUST be rejected.
pub fn verify_handle(
    handle: &str,
    txt_values: &[String],
    also_known_as: &[String],
    expected_did: &str,
) -> Result<()> {
    let txt_did = did_from_txt(txt_values)
        .ok_or_else(|| Error::Handle(format!("no DID in TXT at {}", txt_name(handle))))?;
    if txt_did != expected_did {
        return Err(Error::Handle("TXT DID does not match identity".into()));
    }
    if !also_known_as.iter().any(|a| a == handle) {
        return Err(Error::Handle(
            "one-sided claim: alsoKnownAs does not list the handle".into(),
        ));
    }
    Ok(())
}

/// Parse well-known body: a sole DID line, or JSON `{"did":"did:sol:…"}`.
pub fn parse_well_known(body: &str) -> Result<String> {
    let trimmed = body.trim();
    if trimmed.starts_with('{') {
        let v: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|e| Error::Handle(format!("json: {e}")))?;
        v.get("did")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or_else(|| Error::Handle("well-known JSON missing did".into()))
    } else {
        let line = trimmed.lines().next().unwrap_or("").trim();
        if line.starts_with("did:sol:") {
            Ok(line.to_string())
        } else {
            Err(Error::Handle("well-known body is not a DID".into()))
        }
    }
}

/// Spec 13: `.sol` → SNS; else TXT `_tangled.h`, else well-known.
pub fn lookup_handle(handle: &str, resolver: &HandleResolver) -> Result<HandleProof> {
    if is_sns_handle(handle) {
        return lookup_sns(handle, resolver);
    }
    let qname = txt_name(handle);
    let wk = well_known_url(handle);

    let txt_vals = if let Some(v) = resolver.txt.get(&qname) {
        Some(v.clone())
    } else if resolver.live {
        live_txt(&qname).ok()
    } else {
        None
    };

    let wk_body = if let Some(b) = resolver.well_known.get(&wk) {
        Some(b.clone())
    } else if resolver.live {
        live_well_known(&wk).ok()
    } else {
        None
    };

    let txt_did = txt_vals.as_ref().and_then(|v| did_from_txt(v));
    let txt_head = txt_vals.as_ref().and_then(|v| head_from_txt(v));
    let wk_did = wk_body.as_deref().and_then(|b| parse_well_known(b).ok());

    match (txt_did, wk_did) {
        (Some(a), Some(b)) if a != b => Err(Error::Handle("TXT and well-known disagree".into())),
        (Some(did), _) => Ok(HandleProof {
            did,
            head: txt_head,
        }),
        (None, Some(did)) => Ok(HandleProof {
            did,
            head: txt_head,
        }),
        (None, None) => Err(Error::Handle(format!("no DID for handle {handle}"))),
    }
}

/// Bidirectional resolve: proof DID plus `alsoKnownAs` from `sys/id`.
pub fn resolve_handle(
    handle: &str,
    resolver: &HandleResolver,
    also_known_as: &[String],
) -> Result<HandleProof> {
    let proof = lookup_handle(handle, resolver)?;
    let txt_vals = resolver
        .txt
        .get(&txt_name(handle))
        .cloned()
        .unwrap_or_else(|| vec![proof.did.clone()]);
    verify_handle(handle, &txt_vals, also_known_as, &proof.did)?;
    Ok(proof)
}

/// `alice.sol` (and `*.sol`) is SNS, not DNS.
pub fn is_sns_handle(handle: &str) -> bool {
    let h = handle.trim();
    let lower = h.to_ascii_lowercase();
    lower.ends_with(".sol") && !h.contains('/') && !h.contains(':')
}

pub fn sns_label(handle: &str) -> &str {
    handle
        .strip_suffix(".sol")
        .or_else(|| handle.strip_suffix(".SOL"))
        .unwrap_or(handle)
}

fn cluster_from_rpc(rpc: Option<&str>) -> Cluster {
    match rpc {
        Some(u) if u.contains("devnet") => Cluster::Devnet,
        Some(u) if u.contains("testnet") => Cluster::Testnet,
        _ => Cluster::Mainnet,
    }
}

fn lookup_sns(handle: &str, resolver: &HandleResolver) -> Result<HandleProof> {
    let key = handle.to_ascii_lowercase();
    if let Some(rec) = resolver.sns.get(&key).or_else(|| resolver.sns.get(handle)) {
        return sns_proof(rec, resolver);
    }
    if resolver.live {
        if let Some(rpc) = resolver.solana_rpc.as_deref() {
            let rec = live_sns(&key, rpc)?;
            return sns_proof(&rec, resolver);
        }
    }
    Err(Error::Handle(format!("no SNS record for {handle}")))
}

fn sns_proof(rec: &SnsRecord, resolver: &HandleResolver) -> Result<HandleProof> {
    let did = if let Some(d) = &rec.tangled {
        d.clone()
    } else {
        Did {
            cluster: cluster_from_rpc(resolver.solana_rpc.as_deref()),
            pubkey: rec.owner,
        }
        .as_str()
    };
    Ok(HandleProof {
        did,
        head: rec.head.clone(),
    })
}

fn sha256_32(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).into()
}

/// `sha256("SPL Name Service" || label)`.
pub fn hashed_sns_name(label: &str) -> [u8; 32] {
    let mut v = Vec::with_capacity(SNS_HASH_PREFIX.len() + label.len());
    v.extend_from_slice(SNS_HASH_PREFIX);
    v.extend_from_slice(label.as_bytes());
    sha256_32(&v)
}

fn on_ed25519_curve(pk: &[u8; 32]) -> bool {
    curve25519_dalek::edwards::CompressedEdwardsY(*pk)
        .decompress()
        .is_some()
}

fn find_program_address(seeds: &[&[u8]], program: &[u8; 32]) -> [u8; 32] {
    for bump in (0..=255u8).rev() {
        let mut data = Vec::new();
        for s in seeds {
            data.extend_from_slice(s);
        }
        data.push(bump);
        data.extend_from_slice(program);
        data.extend_from_slice(b"ProgramDerivedAddress");
        let h = sha256_32(&data);
        if !on_ed25519_curve(&h) {
            return h;
        }
    }
    [0u8; 32]
}

/// Derive the `.sol` name registry PDA for `label` (without `.sol`).
pub fn sns_name_account(label: &str) -> Result<[u8; 32]> {
    let hashed = hashed_sns_name(label);
    let program = decode_pubkey_base58(SNS_NAME_PROGRAM)
        .map_err(|e| Error::Handle(format!("SNS program: {e}")))?;
    let root = decode_pubkey_base58(SNS_ROOT_DOMAIN)
        .map_err(|e| Error::Handle(format!("SNS root: {e}")))?;
    let class = [0u8; 32];
    Ok(find_program_address(&[&hashed, &class, &root], &program))
}

/// Name registry: parent (32) || owner (32) || class (32) || data.
pub fn parse_sns_account(data: &[u8]) -> Result<SnsRecord> {
    if data.len() < SNS_HEADER_LEN {
        return Err(Error::Handle("SNS account too short".into()));
    }
    let mut owner = [0u8; 32];
    owner.copy_from_slice(&data[32..64]);
    let rest = std::str::from_utf8(&data[SNS_HEADER_LEN..]).unwrap_or("");
    Ok(SnsRecord {
        owner,
        tangled: did_from_sns_data(rest),
        head: head_from_sns_data(rest),
    })
}

fn did_from_sns_data(s: &str) -> Option<String> {
    let trimmed = s.trim_matches('\0').trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(d) = v.get("tangled").and_then(|x| x.as_str()) {
            return Some(d.to_string());
        }
        if let Some(d) = v.get("did").and_then(|x| x.as_str()) {
            return Some(d.to_string());
        }
    }
    for line in trimmed.split(['\n', ';', '\0']) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("tangled=") {
            return Some(rest.trim().to_string());
        }
        if line.starts_with("did:sol:") {
            return Some(line.to_string());
        }
    }
    None
}

fn head_from_sns_data(s: &str) -> Option<String> {
    let trimmed = s.trim_matches('\0').trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(h) = v.get("head").and_then(|x| x.as_str()) {
            return Some(h.to_string());
        }
    }
    for line in trimmed.split(['\n', ';', '\0']) {
        if let Some(rest) = line.trim().strip_prefix("head=") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

/// Parse `getAccountInfo` JSON-RPC result to account bytes.
pub fn parse_get_account_info(body: &str) -> Result<Vec<u8>> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| Error::Handle(format!("rpc json: {e}")))?;
    let value = v
        .get("result")
        .and_then(|r| r.get("value"))
        .ok_or_else(|| Error::Handle("getAccountInfo missing value".into()))?;
    if value.is_null() {
        return Err(Error::Handle("SNS name not found".into()));
    }
    let data = value
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|s| s.as_str())
        .ok_or_else(|| Error::Handle("getAccountInfo data".into()))?;
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| Error::Handle(format!("sns b64: {e}")))
}

/// Live JSON-RPC `getAccountInfo` for an SNS `.sol` handle. Tests and CI
/// point `rpc` at `solana-test-validator`; production uses `SOLANA_RPC`.
pub fn live_sns(handle: &str, rpc: &str) -> Result<SnsRecord> {
    let label = sns_label(handle);
    let account = sns_name_account(label)?;
    let b58 = encode_pubkey_base58(&account);
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [b58, {"encoding": "base64"}]
    });
    let body = http_post_json(rpc, &req)?;
    parse_sns_account(&parse_get_account_info(&body)?)
}

fn http_post_json(url: &str, body: &serde_json::Value) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| Error::Handle(e.to_string()))?;
    let resp = client
        .post(url)
        .json(body)
        .send()
        .map_err(|e| Error::Handle(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(Error::Handle(format!("rpc {}", resp.status())));
    }
    resp.text().map_err(|e| Error::Handle(e.to_string()))
}

fn live_txt(qname: &str) -> Result<Vec<String>> {
    // ponytail: DoH over existing reqwest rather than a DNS crate. Same-host HTTPS follows.
    let url = format!("https://cloudflare-dns.com/dns-query?name={qname}&type=TXT");
    let body = http_get_same_host(&url, Some("application/dns-json"))?;
    parse_doh_txt(&body)
}

fn live_well_known(url: &str) -> Result<String> {
    http_get_same_host(url, None)
}

fn http_get_same_host(url: &str, accept: Option<&str>) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let prev = attempt.previous().last().and_then(|u| u.host_str());
            let next = attempt.url().host_str();
            if prev == next {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| Error::Handle(e.to_string()))?;
    let mut req = client.get(url);
    if let Some(a) = accept {
        req = req.header("Accept", a);
    }
    let resp = req.send().map_err(|e| Error::Handle(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(Error::Handle(format!("http {}", resp.status())));
    }
    resp.text().map_err(|e| Error::Handle(e.to_string()))
}

fn parse_doh_txt(body: &str) -> Result<Vec<String>> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| Error::Handle(format!("doh json: {e}")))?;
    let mut out = Vec::new();
    if let Some(answers) = v.get("Answer").and_then(|a| a.as_array()) {
        for a in answers {
            if a.get("type").and_then(|t| t.as_u64()) != Some(16) {
                continue;
            }
            if let Some(data) = a.get("data").and_then(|d| d.as_str()) {
                out.push(data.trim_matches('"').to_string());
            }
        }
    }
    if out.is_empty() {
        return Err(Error::Handle("no TXT answers".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_sided_reject() {
        let err = verify_handle(
            "alice.example.com",
            &["did:sol:abc".into()],
            &[],
            "did:sol:abc",
        );
        assert!(err.is_err());
    }

    #[test]
    fn also_known_as_mismatch() {
        let err = verify_handle(
            "alice.example.com",
            &["did:sol:abc".into()],
            &["other.example.com".into()],
            "did:sol:abc",
        );
        assert!(err.is_err());
        let err = verify_handle(
            "alice.example.com",
            &["did:sol:other".into()],
            &["alice.example.com".into()],
            "did:sol:abc",
        );
        assert!(err.is_err());
    }

    #[test]
    fn injected_txt_resolves_and_one_sided_fails() {
        let mut r = HandleResolver::default();
        r.txt
            .insert(txt_name("alice.example.com"), vec!["did:sol:abc".into()]);
        let proof = lookup_handle("alice.example.com", &r).unwrap();
        assert_eq!(proof.did, "did:sol:abc");
        assert!(resolve_handle("alice.example.com", &r, &[]).is_err());
        assert!(resolve_handle("alice.example.com", &r, &["alice.example.com".into()]).is_ok());
    }

    #[test]
    fn txt_and_well_known_must_agree() {
        let mut r = HandleResolver::default();
        r.txt
            .insert(txt_name("alice.example.com"), vec!["did:sol:abc".into()]);
        r.well_known
            .insert(well_known_url("alice.example.com"), "did:sol:other".into());
        assert!(lookup_handle("alice.example.com", &r).is_err());
    }

    #[test]
    fn sns_injected_resolves_and_one_sided_fails() {
        let owner = [7u8; 32];
        let did = Did {
            cluster: Cluster::Mainnet,
            pubkey: owner,
        }
        .as_str();
        let mut r = HandleResolver::default();
        r.sns.insert(
            "alice.sol".into(),
            SnsRecord {
                owner,
                tangled: None,
                head: Some("bafkhead".into()),
            },
        );
        let proof = lookup_handle("alice.sol", &r).unwrap();
        assert_eq!(proof.did, did);
        assert_eq!(proof.head.as_deref(), Some("bafkhead"));
        assert!(resolve_handle("alice.sol", &r, &[]).is_err());
        assert!(resolve_handle("alice.sol", &r, &["alice.sol".into()]).is_ok());
    }

    #[test]
    fn sns_tangled_field_beats_owner() {
        let mut r = HandleResolver::default();
        r.sns.insert(
            "bob.sol".into(),
            SnsRecord {
                owner: [1u8; 32],
                tangled: Some("did:sol:abc".into()),
                head: None,
            },
        );
        let proof = lookup_handle("bob.sol", &r).unwrap();
        assert_eq!(proof.did, "did:sol:abc");
    }

    #[test]
    fn sns_account_bytes_and_rpc_json() {
        let mut raw = vec![0u8; 96];
        raw[32..64].copy_from_slice(&[9u8; 32]);
        raw.extend_from_slice(br#"{"tangled":"did:sol:xyz","head":"bafk1"}"#);
        let rec = parse_sns_account(&raw).unwrap();
        assert_eq!(rec.owner, [9u8; 32]);
        assert_eq!(rec.tangled.as_deref(), Some("did:sol:xyz"));
        assert_eq!(rec.head.as_deref(), Some("bafk1"));

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        let rpc =
            format!(r#"{{"jsonrpc":"2.0","result":{{"value":{{"data":["{b64}","base64"]}}}}}}"#);
        assert_eq!(parse_get_account_info(&rpc).unwrap(), raw);
        let a = sns_name_account("alice").unwrap();
        let b = sns_name_account("alice").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, sns_name_account("bob").unwrap());
        assert!(is_sns_handle("alice.sol"));
        assert!(!is_sns_handle("alice.example.com"));
    }
}
