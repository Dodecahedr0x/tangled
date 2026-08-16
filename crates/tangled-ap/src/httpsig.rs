//! Draft-Cavage HTTP Signatures (`rsa-sha256`) as spoken by Mastodon S2S.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rsa::pkcs1v15::{SigningKey, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256 as Sha256Hash};

use crate::error::{Error, Result};

pub const DEFAULT_MAX_SKEW_SECS: u64 = 300;

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn digest_header(body: &[u8]) -> String {
    let hash = Sha256Hash::digest(body);
    format!("SHA-256={}", STANDARD.encode(hash))
}

pub fn http_date(unix: u64) -> String {
    let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d, wd) = ymd_weekday(unix);
    let h = (unix / 3600) % 24;
    let min = (unix / 60) % 60;
    let s = unix % 60;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        weekdays[wd],
        d,
        months[m as usize - 1],
        y,
        h,
        min,
        s
    )
}

/// Sign a POST/GET. Returns `(Signature, Digest)` headers (Digest empty on GET).
pub fn sign_request(
    key: &RsaPrivateKey,
    key_id: &str,
    method: &str,
    path_and_query: &str,
    host: &str,
    date: &str,
    body: &[u8],
) -> (String, String) {
    let method = method.to_ascii_lowercase();
    let digest = if method == "post" {
        digest_header(body)
    } else {
        String::new()
    };
    let headers = if method == "post" {
        "(request-target) host date digest"
    } else {
        "(request-target) host date"
    };
    let signing = signing_string(&method, path_and_query, host, date, digest.as_str());
    let signing_key = SigningKey::<Sha256>::new(key.clone());
    let sig = signing_key.sign(signing.as_bytes());
    let signature = format!(
        "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"{headers}\",signature=\"{}\"",
        STANDARD.encode(sig.to_bytes())
    );
    (signature, digest)
}

/// Sign the Signature `headers` list in listed order (draft-cavage).
pub fn sign_header_list(
    key: &RsaPrivateKey,
    key_id: &str,
    method: &str,
    path_and_query: &str,
    signed: &str,
    headers: &[(String, String)],
) -> Result<String> {
    let method = method.to_ascii_lowercase();
    let tokens = header_tokens(signed);
    let signing = signing_string_from_list(&method, path_and_query, headers, &tokens)?;
    let signing_key = SigningKey::<Sha256>::new(key.clone());
    let sig = signing_key.sign(signing.as_bytes());
    Ok(format!(
        "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"{signed}\",signature=\"{}\"",
        STANDARD.encode(sig.to_bytes())
    ))
}

pub fn verify_request(
    public_key: &RsaPublicKey,
    method: &str,
    path_and_query: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<()> {
    verify_request_at(
        public_key,
        method,
        path_and_query,
        headers,
        body,
        unix_now(),
        DEFAULT_MAX_SKEW_SECS,
    )
}

pub fn verify_request_at(
    public_key: &RsaPublicKey,
    method: &str,
    path_and_query: &str,
    headers: &[(String, String)],
    body: &[u8],
    now: u64,
    max_skew_secs: u64,
) -> Result<()> {
    let sig_header = header(headers, "signature")
        .ok_or_else(|| Error::Signature("missing Signature header".into()))?;
    let parsed = parse_signature(sig_header)?;
    if parsed.algorithm != "rsa-sha256" && !parsed.algorithm.is_empty() {
        return Err(Error::Signature(format!(
            "unsupported algorithm {}",
            parsed.algorithm
        )));
    }
    let method = method.to_ascii_lowercase();
    let signed = header_tokens(&parsed.headers);
    require_signed(&signed, "(request-target)")?;
    require_signed(&signed, "host")?;
    require_signed(&signed, "date")?;
    if method == "post" {
        require_signed(&signed, "digest")?;
        let digest = header(headers, "digest")
            .ok_or_else(|| Error::Signature("missing Digest header".into()))?;
        let expected = digest_header(body);
        if !digest.eq_ignore_ascii_case(&expected) {
            return Err(Error::Signature("digest mismatch".into()));
        }
    }
    let date_raw =
        header(headers, "date").ok_or_else(|| Error::Signature("missing Date".into()))?;
    let date_unix =
        parse_http_date(date_raw).ok_or_else(|| Error::Signature("invalid Date".into()))?;
    let skew = now.abs_diff(date_unix);
    if skew > max_skew_secs {
        return Err(Error::Signature("date skew".into()));
    }
    let signing = signing_string_from_list(&method, path_and_query, headers, &signed)?;
    let sig_bytes = STANDARD
        .decode(parsed.signature.as_bytes())
        .map_err(|e| Error::Signature(format!("signature b64: {e}")))?;
    let verifying = VerifyingKey::<Sha256>::new(public_key.clone());
    let signature = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| Error::Signature(format!("signature: {e}")))?;
    verifying
        .verify(signing.as_bytes(), &signature)
        .map_err(|_| Error::Signature("rsa-sha256 verify failed".into()))?;
    Ok(())
}

pub fn parse_signature(header: &str) -> Result<ParsedSignature> {
    let mut key_id = String::new();
    let mut algorithm = String::new();
    let mut headers = String::new();
    let mut signature = String::new();
    for part in header.split(',') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"');
        match k.trim() {
            "keyId" => key_id = v.to_string(),
            "algorithm" => algorithm = v.to_string(),
            "headers" => headers = v.to_string(),
            "signature" => signature = v.to_string(),
            _ => {}
        }
    }
    if signature.is_empty() {
        return Err(Error::Signature("missing signature field".into()));
    }
    Ok(ParsedSignature {
        key_id,
        algorithm,
        headers,
        signature,
    })
}

#[derive(Clone, Debug)]
pub struct ParsedSignature {
    pub key_id: String,
    pub algorithm: String,
    pub headers: String,
    pub signature: String,
}

fn signing_string(method: &str, path: &str, host: &str, date: &str, digest: &str) -> String {
    if digest.is_empty() {
        format!("(request-target): {method} {path}\nhost: {host}\ndate: {date}")
    } else {
        format!("(request-target): {method} {path}\nhost: {host}\ndate: {date}\ndigest: {digest}")
    }
}

fn header_tokens(list: &str) -> Vec<String> {
    list.split_whitespace()
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn require_signed(tokens: &[String], name: &str) -> Result<()> {
    if tokens.iter().any(|t| t == name) {
        Ok(())
    } else {
        Err(Error::Signature(format!("signed headers missing {name}")))
    }
}

fn signing_string_from_list(
    method: &str,
    path: &str,
    headers: &[(String, String)],
    signed: &[String],
) -> Result<String> {
    let mut lines = Vec::with_capacity(signed.len());
    for name in signed {
        if name == "(request-target)" {
            lines.push(format!("(request-target): {method} {path}"));
            continue;
        }
        let value = header(headers, name)
            .ok_or_else(|| Error::Signature(format!("missing signed header {name}")))?;
        lines.push(format!("{name}: {value}"));
    }
    Ok(lines.join("\n"))
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

pub fn parse_http_date(s: &str) -> Option<u64> {
    let s = s.trim();
    let rest = s.split_once(", ")?.1;
    let mut parts = rest.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let mon = month_num(parts.next()?)?;
    let year: i32 = parts.next()?.parse().ok()?;
    let time = parts.next()?;
    let tz = parts.next()?;
    if !tz.eq_ignore_ascii_case("GMT") && !tz.eq_ignore_ascii_case("UTC") {
        return None;
    }
    let mut hm = time.split(':');
    let h: u64 = hm.next()?.parse().ok()?;
    let min: u64 = hm.next()?.parse().ok()?;
    let sec: u64 = hm.next()?.parse().ok()?;
    if h > 23 || min > 59 || sec > 60 || day == 0 || day > 31 {
        return None;
    }
    let days = days_from_civil(year, mon, day)?;
    Some(days.checked_mul(86_400)? + h * 3600 + min * 60 + sec)
}

fn month_num(name: &str) -> Option<u32> {
    Some(match name {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

fn days_from_civil(y: i32, m: u32, d: u32) -> Option<u64> {
    if !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let z = era as i64 * 146_097 + doe as i64 - 719_468;
    u64::try_from(z).ok()
}

fn ymd_weekday(secs: u64) -> (i32, u32, u32, usize) {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let wd = ((secs / 86_400) + 4) % 7;
    (y as i32, m as u32, d as u32, wd as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use rsa::RsaPrivateKey;

    fn keypair() -> (RsaPrivateKey, RsaPublicKey) {
        let key = RsaPrivateKey::new(&mut OsRng, 2048).unwrap();
        let pubk = RsaPublicKey::from(&key);
        (key, pubk)
    }

    fn sign_named(
        key: &RsaPrivateKey,
        key_id: &str,
        method: &str,
        path: &str,
        signed: &str,
        pairs: &[(&str, &str)],
    ) -> String {
        let method = method.to_ascii_lowercase();
        let mut lines = Vec::new();
        for name in signed.split_whitespace() {
            if name == "(request-target)" {
                lines.push(format!("(request-target): {method} {path}"));
            } else {
                let v = pairs
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(name))
                    .map(|(_, v)| *v)
                    .unwrap_or("");
                lines.push(format!("{name}: {v}"));
            }
        }
        let signing = lines.join("\n");
        let signing_key = SigningKey::<Sha256>::new(key.clone());
        let sig = signing_key.sign(signing.as_bytes());
        format!(
            "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"{signed}\",signature=\"{}\"",
            STANDARD.encode(sig.to_bytes())
        )
    }

    #[test]
    fn sign_and_verify_fixture() {
        let (key, pubk) = keypair();
        let body = br#"{"type":"Create"}"#;
        let now = unix_now();
        let date = http_date(now);
        let (signature, digest) = sign_request(
            &key,
            "https://ex.example/users/a#main-key",
            "POST",
            "/inbox",
            "knot.example",
            &date,
            body,
        );
        assert!(digest.starts_with("SHA-256="));
        let headers = vec![
            ("host".into(), "knot.example".into()),
            ("date".into(), date),
            ("digest".into(), digest),
            ("signature".into(), signature),
        ];
        verify_request(&pubk, "POST", "/inbox", &headers, body).unwrap();
        assert!(verify_request(&pubk, "POST", "/inbox", &headers, b"tamper").is_err());
    }

    #[test]
    fn post_rejects_signature_that_omits_digest_from_headers_list() {
        let (key, pubk) = keypair();
        let body = br#"{"type":"Create"}"#;
        let now = unix_now();
        let date = http_date(now);
        let digest = digest_header(body);
        let signed = "(request-target) host date";
        let signature = sign_named(
            &key,
            "https://ex.example/users/a#main-key",
            "POST",
            "/inbox",
            signed,
            &[("host", "knot.example"), ("date", &date)],
        );
        let headers = vec![
            ("host".into(), "knot.example".into()),
            ("date".into(), date),
            ("digest".into(), digest),
            ("signature".into(), signature),
        ];
        let err = verify_request(&pubk, "POST", "/inbox", &headers, body).unwrap_err();
        assert!(
            err.to_string().to_ascii_lowercase().contains("digest"),
            "{err}"
        );
    }

    #[test]
    fn post_rejects_stale_date() {
        let (key, pubk) = keypair();
        let body = br#"{"type":"Create"}"#;
        let date = http_date(1_700_000_000);
        let (signature, digest) = sign_request(
            &key,
            "https://ex.example/users/a#main-key",
            "POST",
            "/inbox",
            "knot.example",
            &date,
            body,
        );
        let headers = vec![
            ("host".into(), "knot.example".into()),
            ("date".into(), date),
            ("digest".into(), digest),
            ("signature".into(), signature),
        ];
        let err = verify_request(&pubk, "POST", "/inbox", &headers, body).unwrap_err();
        assert!(
            err.to_string().to_ascii_lowercase().contains("date"),
            "{err}"
        );
    }

    #[test]
    fn post_rejects_missing_date() {
        let (key, pubk) = keypair();
        let body = br#"{"type":"Create"}"#;
        let now = unix_now();
        let date = http_date(now);
        let digest = digest_header(body);
        let signature = sign_named(
            &key,
            "https://ex.example/users/a#main-key",
            "POST",
            "/inbox",
            "(request-target) host digest",
            &[("host", "knot.example"), ("digest", &digest)],
        );
        let headers = vec![
            ("host".into(), "knot.example".into()),
            ("digest".into(), digest),
            ("signature".into(), signature),
        ];
        let _ = date;
        assert!(verify_request(&pubk, "POST", "/inbox", &headers, body).is_err());
    }

    #[test]
    fn signing_string_follows_headers_list_including_content_type() {
        let (key, pubk) = keypair();
        let body = br#"{"type":"Create"}"#;
        let now = unix_now();
        let date = http_date(now);
        let digest = digest_header(body);
        let ct = "application/activity+json";
        let signed = "(request-target) host date digest content-type";
        let signature = sign_named(
            &key,
            "https://ex.example/users/a#main-key",
            "POST",
            "/inbox",
            signed,
            &[
                ("host", "knot.example"),
                ("date", &date),
                ("digest", &digest),
                ("content-type", ct),
            ],
        );
        let headers = vec![
            ("host".into(), "knot.example".into()),
            ("date".into(), date),
            ("digest".into(), digest),
            ("content-type".into(), ct.into()),
            ("signature".into(), signature),
        ];
        verify_request(&pubk, "POST", "/inbox", &headers, body).unwrap();
    }

    #[test]
    fn digest_token_is_not_a_substring_match() {
        let (key, pubk) = keypair();
        let body = br#"{"type":"Create"}"#;
        let now = unix_now();
        let date = http_date(now);
        let digest = digest_header(body);
        let signed = "(request-target) host date content-digest";
        let signature = sign_named(
            &key,
            "https://ex.example/users/a#main-key",
            "POST",
            "/inbox",
            signed,
            &[
                ("host", "knot.example"),
                ("date", &date),
                ("content-digest", &digest),
            ],
        );
        let headers = vec![
            ("host".into(), "knot.example".into()),
            ("date".into(), date),
            ("digest".into(), digest.clone()),
            ("content-digest".into(), digest),
            ("signature".into(), signature),
        ];
        assert!(verify_request(&pubk, "POST", "/inbox", &headers, body).is_err());
    }

    #[test]
    fn http_date_roundtrip() {
        for t in [0u64, 1, 1_700_000_000, 1_800_000_000] {
            assert_eq!(parse_http_date(&http_date(t)), Some(t), "t={t}");
        }
    }
}
