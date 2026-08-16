//! RFC 9421 HTTP Message Signatures (ed25519) + RFC 9530 Content-Digest (sha-256).
//!
//! Normative covered set: `@method`, `@path`, `content-digest` (MUST).
//! `@authority` (or `host`) and `content-type` MUST verify when listed.
//! Other RFC 9421 features MAY be ignored.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// RFC 9530 `sha-256` Content-Digest of `body`.
pub fn content_digest_sha256(body: &[u8]) -> String {
    let hash = Sha256::digest(body);
    format!("sha-256=:{}:", STANDARD.encode(hash))
}

/// Sign `@method` `@path` `content-digest` with `ed25519`.
///
/// Returns `(signature-input, signature, content-digest)`.
pub fn sign_ed25519(
    sk: &SigningKey,
    keyid: &str,
    method: &str,
    path: &str,
    body: &[u8],
) -> (String, String, String) {
    sign_ed25519_ext(sk, keyid, method, path, body, &[])
}

/// Sign the required components plus extra covered `(name, value)` pairs.
pub fn sign_ed25519_ext(
    sk: &SigningKey,
    keyid: &str,
    method: &str,
    path: &str,
    body: &[u8],
    extra: &[(&str, &str)],
) -> (String, String, String) {
    let digest = content_digest_sha256(body);
    let mut names = vec!["@method", "@path", "content-digest"];
    names.extend(extra.iter().map(|(n, _)| *n));
    let listed = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let params = format!(r#"({listed});alg="ed25519";keyid="{keyid}""#);
    let input = format!("sig1={params}");
    let mut headers: Vec<(String, String)> = extra
        .iter()
        .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
        .collect();
    if let Some((_, v)) = extra
        .iter()
        .find(|(n, _)| *n == "@authority" || *n == "host")
    {
        headers.push(("host".into(), (*v).to_string()));
    }
    let base = signature_base(method, path, &digest, &params, &headers)
        .expect("sign covers only the implemented component set");
    let sig = sk.sign(base.as_bytes());
    let header = format!("sig1=:{}:", STANDARD.encode(sig.to_bytes()));
    (input, header, digest)
}

/// Verify RFC 9421 `ed25519` over `@method` `@path` `content-digest`.
pub fn verify_ed25519(
    pk: &[u8; 32],
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<()> {
    let input = header(headers, "signature-input")
        .ok_or_else(|| Error::Foreign("missing Signature-Input".into()))?;
    let sig_h =
        header(headers, "signature").ok_or_else(|| Error::Foreign("missing Signature".into()))?;
    let digest = header(headers, "content-digest")
        .ok_or_else(|| Error::Foreign("missing Content-Digest".into()))?;
    let expected = content_digest_sha256(body);
    if !digest.eq_ignore_ascii_case(&expected) {
        return Err(Error::Foreign("Content-Digest mismatch".into()));
    }
    let params = inner_params(input)?;
    require_covered(&params, "@method")?;
    require_covered(&params, "@path")?;
    require_covered(&params, "content-digest")?;
    if !params.to_ascii_lowercase().contains("alg=\"ed25519\"")
        && !params.to_ascii_lowercase().contains("alg=ed25519")
    {
        return Err(Error::Foreign(
            "HTTP Message Signature alg MUST be ed25519".into(),
        ));
    }
    let b64 = signature_b64(sig_h)?;
    let bytes = STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| Error::Foreign(format!("signature b64: {e}")))?;
    let sig_arr: [u8; 64] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| Error::Foreign("signature must be 64 bytes".into()))?;
    let vk = VerifyingKey::from_bytes(pk).map_err(|_| Error::InvalidSignature)?;
    let base = signature_base(method, path, digest, &params, headers)?;
    vk.verify(base.as_bytes(), &Signature::from_bytes(&sig_arr))
        .map_err(|_| Error::InvalidSignature)
}

/// True when RFC 9421 headers (or stored field names) are present.
pub fn has_message_signature(headers: &[(String, String)]) -> bool {
    header(headers, "signature-input").is_some() && header(headers, "signature").is_some()
}

/// Stored `foreignProof` fields that stand in for HTTP Message Signature headers.
pub fn headers_from_proof(proof: &serde_json::Value) -> Vec<(String, String)> {
    let mut h = Vec::new();
    for (name, keys) in [
        (
            "signature-input",
            &["signature-input", "signatureInput"][..],
        ),
        ("signature", &["signature"][..]),
        ("content-digest", &["content-digest", "contentDigest"][..]),
        ("content-type", &["content-type", "contentType"][..]),
        ("host", &["host", "@authority", "authority"][..]),
    ] {
        for k in keys {
            if let Some(s) = proof.get(*k).and_then(|v| v.as_str()) {
                h.push((name.into(), s.to_string()));
                break;
            }
        }
    }
    h
}

pub fn proof_has_message_signature(proof: &serde_json::Value) -> bool {
    has_message_signature(&headers_from_proof(proof))
}

fn signature_base(
    method: &str,
    path: &str,
    digest: &str,
    params: &str,
    headers: &[(String, String)],
) -> Result<String> {
    let mut lines = Vec::new();
    for name in parse_covered(params)? {
        let value = match name.as_str() {
            "@method" => method.to_ascii_uppercase(),
            "@path" => path.to_string(),
            "content-digest" => digest.to_string(),
            "@authority" | "host" => header(headers, "host")
                .or_else(|| header(headers, "@authority"))
                .ok_or_else(|| Error::Foreign("missing @authority/host".into()))?
                .to_string(),
            "content-type" => header(headers, "content-type")
                .ok_or_else(|| Error::Foreign("missing content-type".into()))?
                .to_string(),
            _ => continue, // other RFC 9421 features MAY be ignored
        };
        lines.push(format!("\"{name}\": {value}"));
    }
    lines.push(format!("\"@signature-params\": {params}"));
    Ok(lines.join("\n"))
}

fn parse_covered(params: &str) -> Result<Vec<String>> {
    let inner = params.split(';').next().unwrap_or(params);
    let inner = inner.trim().trim_start_matches('(').trim_end_matches(')');
    Ok(inner
        .split_whitespace()
        .map(|s| s.trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

fn inner_params(input: &str) -> Result<String> {
    let rest = input
        .split_once('=')
        .map(|(_, r)| r.trim())
        .unwrap_or(input.trim());
    if rest.is_empty() {
        return Err(Error::Foreign("empty Signature-Input".into()));
    }
    Ok(rest.to_string())
}

fn require_covered(params: &str, name: &str) -> Result<()> {
    let quoted = format!("\"{name}\"");
    if params.contains(&quoted) {
        Ok(())
    } else {
        Err(Error::Foreign(format!(
            "HTTP Message Signature MUST cover {name}"
        )))
    }
}

fn signature_b64(header: &str) -> Result<String> {
    let rest = header
        .split_once('=')
        .map(|(_, r)| r.trim())
        .unwrap_or(header.trim());
    let s = rest.trim().trim_matches(':');
    if s.is_empty() {
        return Err(Error::Foreign("empty Signature".into()));
    }
    Ok(s.to_string())
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    #[test]
    fn valid_signature_accepted_mutated_body_rejected() {
        let id = Identity::from_seeds([11u8; 32], [12u8; 32]);
        let body = br#"{"id":"https://ex.example/n/1"}"#;
        let (input, sig, digest) = sign_ed25519(
            id.signing_key(),
            &format!("{}#key", id.did_string()),
            "POST",
            "/inbox",
            body,
        );
        let headers = vec![
            ("signature-input".into(), input),
            ("signature".into(), sig),
            ("content-digest".into(), digest),
        ];
        verify_ed25519(&id.pubkey_bytes(), "POST", "/inbox", &headers, body).unwrap();
        assert!(verify_ed25519(&id.pubkey_bytes(), "POST", "/inbox", &headers, b"tamper").is_err());
    }

    #[test]
    fn extra_covered_content_type_mismatch_fails() {
        let id = Identity::from_seeds([11u8; 32], [12u8; 32]);
        let body = br#"{"id":"https://ex.example/n/1"}"#;
        let (input, sig, digest) = sign_ed25519_ext(
            id.signing_key(),
            &format!("{}#key", id.did_string()),
            "POST",
            "/inbox",
            body,
            &[("content-type", "application/activity+json")],
        );
        let headers = vec![
            ("signature-input".into(), input),
            ("signature".into(), sig),
            ("content-digest".into(), digest),
            ("content-type".into(), "text/plain".into()),
        ];
        assert!(verify_ed25519(&id.pubkey_bytes(), "POST", "/inbox", &headers, body).is_err());
        let (input, sig, digest) = sign_ed25519_ext(
            id.signing_key(),
            &format!("{}#key", id.did_string()),
            "POST",
            "/inbox",
            body,
            &[("content-type", "application/activity+json")],
        );
        let ok = vec![
            ("signature-input".into(), input),
            ("signature".into(), sig),
            ("content-digest".into(), digest),
            ("content-type".into(), "application/activity+json".into()),
        ];
        verify_ed25519(&id.pubkey_bytes(), "POST", "/inbox", &ok, body).unwrap();
    }
}
