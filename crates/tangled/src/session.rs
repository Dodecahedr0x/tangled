//! Sign-In With Solana (spec 05/12). Knot-local login, not a record signature.
//!
//! Cookie name is OpenAPI `tangled_session`. Sessions expire after 24 hours.
//! The SIWS message includes knot domain, DID, nonce, and `issuedAt`.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::identity::Did;
use crate::sign;

/// OpenAPI cookie name.
pub const COOKIE_NAME: &str = "tangled_session";
/// Spec 12 RECOMMENDED session lifetime.
pub const SESSION_TTL_SECONDS: u64 = 86_400;
/// Challenge lifetime (not specified; short-lived nonce).
pub const CHALLENGE_TTL_SECONDS: u64 = 300;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiwsChallenge {
    pub domain: String,
    pub did: String,
    pub nonce: String,
    #[serde(rename = "issuedAt")]
    pub issued_at: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiwsVerifyRequest {
    pub did: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiwsVerifyResponse {
    pub ok: bool,
    pub did: String,
    pub exp: u64,
}

#[derive(Clone, Debug)]
pub struct ChallengeRecord {
    pub did: String,
    pub domain: String,
    pub issued_at: u64,
    pub exp: u64,
}

#[derive(Clone, Debug)]
pub struct SessionRecord {
    pub did: String,
    pub exp: u64,
}

/// Canonical SIWS message (spec 12: domain, DID, nonce, issuedAt).
pub fn siws_message(domain: &str, did: &str, nonce: &str, issued_at: u64) -> String {
    format!("tangled.siws.v1\ndomain: {domain}\ndid: {did}\nnonce: {nonce}\nissuedAt: {issued_at}")
}

pub fn new_nonce() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}

pub fn new_session_token() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

/// `Set-Cookie` value (HttpOnly, Path=/, Max-Age=24h, SameSite=Lax).
/// `Secure` when the advertised knot URL is HTTPS (spec 12 production).
pub fn set_cookie_header(token: &str) -> String {
    set_cookie_header_secure(token, false)
}

pub fn set_cookie_header_secure(token: &str, secure: bool) -> String {
    let mut s = format!(
        "{COOKIE_NAME}={token}; HttpOnly; Path=/; Max-Age={SESSION_TTL_SECONDS}; SameSite=Lax"
    );
    if secure {
        s.push_str("; Secure");
    }
    s
}

pub fn parse_session_cookie(headers: &[(String, String)]) -> Option<String> {
    for (k, v) in headers {
        if !k.eq_ignore_ascii_case("cookie") {
            continue;
        }
        for part in v.split(';') {
            let part = part.trim();
            if let Some(rest) = part.strip_prefix(&format!("{COOKIE_NAME}=")) {
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

pub fn verify_siws_signature(did: &str, message: &str, signature_hex: &str) -> Result<()> {
    let parsed = Did::parse(did)?;
    let vk = VerifyingKey::from_bytes(&parsed.pubkey).map_err(|_| Error::InvalidSignature)?;
    let sig_bytes = sign::sig_from_hex(signature_hex)?;
    let signature = Signature::from_bytes(&sig_bytes);
    vk.verify(message.as_bytes(), &signature)
        .map_err(|_| Error::InvalidSignature)
}

pub fn sign_siws(sk: &ed25519_dalek::SigningKey, message: &str) -> String {
    hex::encode(sign::sign_message(sk, message.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    #[test]
    fn message_includes_required_fields() {
        let m = siws_message("localhost", "did:sol:abc", "aa", 1_700_000_000);
        assert!(m.contains("domain: localhost"));
        assert!(m.contains("did: did:sol:abc"));
        assert!(m.contains("nonce: aa"));
        assert!(m.contains("issuedAt: 1700000000"));
    }

    #[test]
    fn good_signature_verifies() {
        let id = Identity::generate();
        let did = id.did_string();
        let msg = siws_message("localhost", &did, "ff", 1);
        let sig = sign_siws(id.signing_key(), &msg);
        verify_siws_signature(&did, &msg, &sig).unwrap();
    }

    #[test]
    fn bad_signature_rejected() {
        let id = Identity::generate();
        let did = id.did_string();
        let msg = siws_message("localhost", &did, "ff", 1);
        let err = verify_siws_signature(&did, &msg, &"00".repeat(64));
        assert!(err.is_err());
    }

    #[test]
    fn parse_cookie_header() {
        let headers = vec![
            ("Cookie".into(), "foo=bar; tangled_session=deadbeef".into()),
            ("Accept".into(), "*/*".into()),
        ];
        assert_eq!(parse_session_cookie(&headers).as_deref(), Some("deadbeef"));
        assert!(parse_session_cookie(&[("cookie".into(), "a=b".into())]).is_none());
        let http = set_cookie_header("tok");
        assert!(!http.contains("Secure"));
        let https = set_cookie_header_secure("tok", true);
        assert!(https.contains("Secure"));
    }
}
