//! Canonical `foreign_id` and idempotent `sys.foreign` rkey (spec 16).

use crate::error::{Error, Result};

/// Protocols with a normative canonical-id table.
pub const KNOWN_PROTOCOLS: &[&str] = &["nostr", "atproto", "activitypub", "farcaster"];

/// Canonical foreign object id (spec 16 table).
pub fn canonical_foreign_id(protocol: &str, raw: &str) -> Result<String> {
    match protocol {
        "nostr" => {
            let s = raw.trim().to_ascii_lowercase();
            if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(Error::Foreign(
                    "nostr foreign_id MUST be lowercase 64-hex event id".into(),
                ));
            }
            Ok(s)
        }
        "atproto" => {
            if !raw.starts_with("at://") || !raw.contains('#') {
                return Err(Error::Foreign(
                    "atproto foreign_id MUST be at://<did>/<collection>/<rkey>#<record-cid>".into(),
                ));
            }
            Ok(raw.to_string())
        }
        "activitypub" => {
            // Scheme http/https MUST be lowercase; the rest copied verbatim.
            if let Some(rest) = raw.strip_prefix("HTTP://") {
                Ok(format!("http://{rest}"))
            } else if let Some(rest) = raw.strip_prefix("HTTPS://") {
                Ok(format!("https://{rest}"))
            } else if let Some(rest) = raw.strip_prefix("Http://") {
                Ok(format!("http://{rest}"))
            } else if let Some(rest) = raw.strip_prefix("Https://") {
                Ok(format!("https://{rest}"))
            } else if raw.starts_with("http://") || raw.starts_with("https://") {
                Ok(raw.to_string())
            } else {
                Err(Error::Foreign(
                    "activitypub foreign_id MUST be an absolute http(s) IRI".into(),
                ))
            }
        }
        "farcaster" => Ok(raw.trim().to_ascii_lowercase()),
        _ => Ok(raw.to_string()),
    }
}

/// rkey = lowercase hex(BLAKE3(UTF-8(protocol) || 0x00 || UTF-8(foreign_id))).
pub fn foreign_rkey(protocol: &str, foreign_id: &str) -> String {
    let mut data = Vec::with_capacity(protocol.len() + 1 + foreign_id.len());
    data.extend_from_slice(protocol.as_bytes());
    data.push(0x00);
    data.extend_from_slice(foreign_id.as_bytes());
    hex::encode(blake3::hash(&data).as_bytes())
}

/// Idempotent import key: canonicalize then rkey.
pub fn import_rkey(protocol: &str, raw_id: &str) -> Result<String> {
    let id = canonical_foreign_id(protocol, raw_id)?;
    Ok(foreign_rkey(protocol, &id))
}

/// ActivityPub imports MUST be gateway-attested (spec 16).
pub fn activitypub_import_attested(attested: bool) -> Result<()> {
    if attested {
        Ok(())
    } else {
        Err(Error::Foreign(
            "ActivityPub imports MUST set attested: true (gateway-attested)".into(),
        ))
    }
}

/// ActivityPub well-known / JSON proof (spec 16). `attested` is still required.
///
/// Matching `{did, actor}` is REQUIRED when those fields are present. HTTP
/// Message Signatures (RFC 9421) are REQUIRED in addition.
pub fn verify_ap_proof(did: &str, actor: &str, proof: &serde_json::Value) -> Result<()> {
    let proof_did = proof.get("did").and_then(|d| d.as_str());
    let proof_actor = proof.get("actor").and_then(|a| a.as_str());
    let has_json = proof_did.is_some() || proof_actor.is_some();
    if has_json && (proof_did != Some(did) || proof_actor != Some(actor)) {
        return Err(Error::Foreign(
            "ActivityPub foreignProof MUST list matching did and actor".into(),
        ));
    }
    if !crate::httpsig::proof_has_message_signature(proof) {
        return Err(Error::Foreign(
            "ActivityPub foreignProof MUST include a verifiable HTTP Message Signature".into(),
        ));
    }
    verify_ap_httpsig(did, proof)
}

/// Gateway-attested `foreignProof` with RFC 9421 over `body`.
pub fn activitypub_foreign_proof(
    id: &crate::identity::Identity,
    actor: &str,
    body: &[u8],
) -> serde_json::Value {
    let did = id.did_string();
    let (input, sig, digest) = crate::httpsig::sign_ed25519(
        id.signing_key(),
        &format!("{did}#key"),
        "POST",
        "/inbox",
        body,
    );
    serde_json::json!({
        "did": did,
        "actor": actor,
        "signatureInput": input,
        "signature": sig,
        "contentDigest": digest,
        "method": "POST",
        "path": "/inbox",
        "body": String::from_utf8_lossy(body),
    })
}

fn proof_body(proof: &serde_json::Value) -> Vec<u8> {
    if let Some(s) = proof.get("body").and_then(|v| v.as_str()) {
        return s.as_bytes().to_vec();
    }
    if let Some(h) = proof.get("bodyHex").and_then(|v| v.as_str()) {
        if let Ok(b) = hex::decode(h) {
            return b;
        }
    }
    Vec::new()
}

/// Verify stored RFC 9421 fields on `foreignProof` with the Tangled identity key.
pub fn verify_ap_httpsig(did: &str, proof: &serde_json::Value) -> Result<()> {
    let pk = crate::identity::Did::parse(did)?.pubkey;
    let method = proof
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("POST");
    let path = proof.get("path").and_then(|p| p.as_str()).unwrap_or("/");
    let headers = crate::httpsig::headers_from_proof(proof);
    let body = proof_body(proof);
    crate::httpsig::verify_ed25519(&pk, method, path, &headers, &body)
}

/// Deterministic Ed25519 + X25519 seeds for a hosted AP actor DID.
pub fn activitypub_actor_seeds(actor: &str) -> ([u8; 32], [u8; 32]) {
    let ed = *blake3::hash(format!("tangled-ap-ed\0{actor}").as_bytes()).as_bytes();
    let x = *blake3::hash(format!("tangled-ap-x\0{actor}").as_bytes()).as_bytes();
    (ed, x)
}

/// Hosted identity for an ActivityPub actor IRI (gateway-attested).
pub fn activitypub_actor_identity(actor: &str) -> crate::identity::Identity {
    let (ed, x) = activitypub_actor_seeds(actor);
    crate::identity::Identity::from_seeds(ed, x)
}

/// Actor IRI + optional `preferredUsername` → `user@host`.
///
/// Path `/users/name` or `/@name` supplies the user when the username is missing.
pub fn activitypub_actor_acct(actor: &str, preferred_username: Option<&str>) -> Option<String> {
    let host = url_authority(actor)?;
    let user = preferred_username
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains('@') && !s.contains('/'))
        .map(str::to_string)
        .or_else(|| actor_path_username(actor))?;
    if user.is_empty() {
        return None;
    }
    Some(format!("{user}@{host}"))
}

fn url_authority(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split('/').next()?.trim();
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}

fn actor_path_username(actor: &str) -> Option<String> {
    let rest = actor
        .strip_prefix("https://")
        .or_else(|| actor.strip_prefix("http://"))?;
    let path = rest.split_once('/')?.1.trim_end_matches('/');
    let name = if let Some(rest) = path.strip_prefix("users/") {
        rest.split('/').next().unwrap_or("")
    } else if let Some(rest) = path.strip_prefix('@') {
        rest.split('/').next().unwrap_or("")
    } else {
        ""
    };
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// `acct:user@host` → `https://host/users/user` (Mastodon-shaped; WebFinger may override).
pub fn activitypub_acct_actor(resource: &str) -> Result<String> {
    let rest = resource
        .strip_prefix("acct:")
        .or_else(|| resource.strip_prefix('@'))
        .unwrap_or(resource)
        .trim();
    let (user, host) = rest
        .split_once('@')
        .ok_or_else(|| Error::Foreign("ActivityPub acct resource MUST be user@host".into()))?;
    if user.is_empty() || host.is_empty() {
        return Err(Error::Foreign(
            "ActivityPub acct resource MUST be user@host".into(),
        ));
    }
    let scheme = if acct_host_is_loopback(host) {
        "http"
    } else {
        "https"
    };
    Ok(format!("{scheme}://{host}/users/{user}"))
}

fn acct_host_is_loopback(host: &str) -> bool {
    let name = acct_hostname(host);
    name.eq_ignore_ascii_case("localhost")
        || name.ends_with(".localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn acct_hostname(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(host);
    }
    if let Some((name, port)) = host.rsplit_once(':') {
        if port.chars().all(|c| c.is_ascii_digit()) && !name.contains(':') {
            return name;
        }
    }
    host
}

/// Binding message bytes: `tangled-binding:v1:<did>:<hex(nonce)>`.
pub fn binding_message(did: &str, nonce: &[u8]) -> Vec<u8> {
    format!("tangled-binding:v1:{did}:{}", hex::encode(nonce)).into_bytes()
}

/// SHA-256 of the binding UTF-8 (input to BIP-340).
pub fn binding_prehash(did: &str, nonce: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let msg = binding_message(did, nonce);
    Sha256::digest(&msg).into()
}

/// BIP-340 Schnorr over SHA-256(binding message). `sk` is a 32-byte secp256k1 scalar.
pub fn nostr_sign_binding(sk: &[u8; 32], did: &str, nonce: &[u8]) -> Result<[u8; 64]> {
    use k256::schnorr::{signature::hazmat::PrehashSigner, SigningKey};
    let bytes = k256::FieldBytes::from(*sk);
    let key =
        SigningKey::from_bytes(&bytes).map_err(|e| Error::Foreign(format!("nostr sk: {e}")))?;
    let prehash = binding_prehash(did, nonce);
    let sig = key
        .sign_prehash(&prehash)
        .map_err(|e| Error::Foreign(format!("bip-340: {e}")))?;
    Ok(sig.to_bytes())
}

/// X-only secp256k1 public key for a Nostr binding secret.
pub fn nostr_xonly_pk(sk: &[u8; 32]) -> Result<[u8; 32]> {
    use k256::schnorr::SigningKey;
    let bytes = k256::FieldBytes::from(*sk);
    let key =
        SigningKey::from_bytes(&bytes).map_err(|e| Error::Foreign(format!("nostr sk: {e}")))?;
    let pk = key.verifying_key().to_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&pk);
    Ok(out)
}

/// Verify BIP-340 over the binding prehash.
pub fn nostr_verify_binding(
    pk_xonly: &[u8; 32],
    did: &str,
    nonce: &[u8],
    sig: &[u8; 64],
) -> Result<()> {
    use k256::schnorr::{signature::hazmat::PrehashVerifier, Signature, VerifyingKey};
    let bytes = k256::FieldBytes::from(*pk_xonly);
    let vk =
        VerifyingKey::from_bytes(&bytes).map_err(|e| Error::Foreign(format!("nostr pk: {e}")))?;
    let signature =
        Signature::try_from(sig.as_slice()).map_err(|e| Error::Foreign(format!("sig: {e}")))?;
    let prehash = binding_prehash(did, nonce);
    vk.verify_prehash(&prehash, &signature)
        .map_err(|_| Error::InvalidSignature)
}

/// Duplicate detection: equal `bytes_cid`, or equal canonical `(protocol, foreign_id)`, or equal `mapped`.
#[derive(Clone, Debug)]
pub struct ForeignEnvelope {
    pub protocol: String,
    pub foreign_id: String,
    pub bytes_cid: Option<String>,
    pub mapped: Option<String>,
}

pub fn same_object(a: &ForeignEnvelope, b: &ForeignEnvelope) -> bool {
    if let (Some(x), Some(y)) = (&a.bytes_cid, &b.bytes_cid) {
        if x == y {
            return true;
        }
    }
    if a.protocol == b.protocol && a.foreign_id == b.foreign_id {
        return true;
    }
    if let (Some(x), Some(y)) = (&a.mapped, &b.mapped) {
        if x == y {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rkey_stable() {
        let a = foreign_rkey("nostr", "aa".repeat(32).as_str());
        let b = foreign_rkey("nostr", "aa".repeat(32).as_str());
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        let c = foreign_rkey("atproto", "aa".repeat(32).as_str());
        assert_ne!(a, c);
    }

    #[test]
    fn nostr_id_case() {
        let hex64 = "ab".repeat(32);
        assert_eq!(
            canonical_foreign_id("nostr", &hex64.to_uppercase()).unwrap(),
            hex64
        );
    }

    #[test]
    fn canonical_ids_all_protocols() {
        let n = canonical_foreign_id("nostr", &"CD".repeat(32)).unwrap();
        assert_eq!(n, "cd".repeat(32));
        let at =
            canonical_foreign_id("atproto", "at://did:plc:x/app.bsky.feed.post/y#bafyabc").unwrap();
        assert!(at.contains('#'));
        assert!(canonical_foreign_id("atproto", "at://did:plc:x/app.bsky.feed.post/y").is_err());
        let ap = canonical_foreign_id("activitypub", "HTTPS://Example.com/users/a").unwrap();
        assert_eq!(ap, "https://Example.com/users/a");
        assert_eq!(
            canonical_foreign_id("farcaster", "AbCdEf").unwrap(),
            "abcdef"
        );
        assert_eq!(
            import_rkey("nostr", &"AB".repeat(32)).unwrap(),
            import_rkey("nostr", &"ab".repeat(32)).unwrap()
        );
    }

    #[test]
    fn actor_iri_to_acct() {
        assert_eq!(
            activitypub_actor_acct("https://mastodon.social/users/Gargron", None).unwrap(),
            "Gargron@mastodon.social"
        );
        assert_eq!(
            activitypub_actor_acct("https://mastodon.social/@Gargron", Some("Gargron")).unwrap(),
            "Gargron@mastodon.social"
        );
        assert_eq!(
            activitypub_actor_acct("http://127.0.0.1:19876/users/demo", None).unwrap(),
            "demo@127.0.0.1:19876"
        );
    }

    #[test]
    fn acct_actor_and_hosted_did() {
        assert_eq!(
            activitypub_acct_actor("acct:alice@example.com").unwrap(),
            "https://example.com/users/alice"
        );
        assert_eq!(
            activitypub_acct_actor("@bob@host.example").unwrap(),
            "https://host.example/users/bob"
        );
        let a = activitypub_actor_identity("https://example.com/users/alice");
        let b = activitypub_actor_identity("https://example.com/users/alice");
        assert_eq!(a.did_string(), b.did_string());
        assert_ne!(
            a.did_string(),
            activitypub_actor_identity("https://example.com/users/bob").did_string()
        );
    }

    #[test]
    fn acct_actor_loopback_uses_http() {
        assert_eq!(
            activitypub_acct_actor("acct:demo@127.0.0.1:19876").unwrap(),
            "http://127.0.0.1:19876/users/demo"
        );
        assert_eq!(
            activitypub_acct_actor("acct:demo@localhost:3001").unwrap(),
            "http://localhost:3001/users/demo"
        );
        assert_eq!(
            activitypub_acct_actor("acct:alice@example.com").unwrap(),
            "https://example.com/users/alice"
        );
    }

    #[test]
    fn activitypub_must_be_attested() {
        assert!(activitypub_import_attested(true).is_ok());
        assert!(activitypub_import_attested(false).is_err());
        assert!(verify_ap_proof(
            "did:sol:x",
            "https://ex.example/users/a",
            &serde_json::json!({"did":"did:sol:x","actor":"https://ex.example/users/a"}),
        )
        .is_err());
        assert!(verify_ap_proof(
            "did:sol:x",
            "https://ex.example/users/a",
            &serde_json::json!({"did":"did:sol:y","actor":"https://ex.example/users/a"}),
        )
        .is_err());
    }

    #[test]
    fn activitypub_httpsig_present_must_verify() {
        let id = crate::identity::Identity::from_seeds([13u8; 32], [14u8; 32]);
        let did = id.did_string();
        let actor = "https://ex.example/users/a";
        let body = br#"{"type":"Create"}"#;
        let (input, sig, digest) = crate::httpsig::sign_ed25519(
            id.signing_key(),
            &format!("{did}#key"),
            "POST",
            "/inbox",
            body,
        );
        let proof = serde_json::json!({
            "did": did,
            "actor": actor,
            "signatureInput": input,
            "signature": sig,
            "contentDigest": digest,
            "method": "POST",
            "path": "/inbox",
            "body": String::from_utf8_lossy(body),
        });
        verify_ap_proof(&did, actor, &proof).unwrap();
        let mut bad = proof.clone();
        bad["body"] = serde_json::json!("tamper");
        assert!(verify_ap_proof(&did, actor, &bad).is_err());
    }

    #[test]
    fn duplicate_envelopes() {
        let a = ForeignEnvelope {
            protocol: "nostr".into(),
            foreign_id: "aa".repeat(32),
            bytes_cid: Some("bafy1".into()),
            mapped: None,
        };
        let b = ForeignEnvelope {
            protocol: "nostr".into(),
            foreign_id: "bb".repeat(32),
            bytes_cid: Some("bafy1".into()),
            mapped: None,
        };
        assert!(same_object(&a, &b));
    }
}
