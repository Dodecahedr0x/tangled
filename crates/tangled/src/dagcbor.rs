//! DAG-CBOR encode/decode matching IPLD DAG-CBOR / RFC 7049 §3.9 canonical maps.
//!
//! Map keys are UTF-8 strings sorted by encoded-key length, then bytes — the
//! order `cborg` used to generate `spec/vectors`. Floats are rejected. The only
//! tag is CID tag 42.

use std::collections::BTreeMap;

use crate::cid::Cid;
use crate::error::{Error, Result};

/// Canonical DAG-CBOR value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Unsigned(u64),
    Negative(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// IPLD CID link (tag 42).
    Cid(Cid),
}

impl Value {
    pub fn map() -> Self {
        Value::Map(BTreeMap::new())
    }

    pub fn insert(&mut self, k: impl Into<String>, v: Value) -> Result<()> {
        match self {
            Value::Map(m) => {
                m.insert(k.into(), v);
                Ok(())
            }
            _ => Err(Error::dagcbor("insert on non-map")),
        }
    }

    pub fn get(&self, k: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.get(k),
            _ => None,
        }
    }
}

/// Encode `value` as canonical DAG-CBOR.
pub fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_value(&mut out, value)?;
    Ok(out)
}

/// Encode and return CIDv1 dag-cbor blake3.
pub fn cid_of(value: &Value) -> Result<Cid> {
    Ok(Cid::dag_cbor(&encode(value)?))
}

fn write_value(out: &mut Vec<u8>, value: &Value) -> Result<()> {
    match value {
        Value::Null => out.push(0xf6),
        Value::Bool(false) => out.push(0xf4),
        Value::Bool(true) => out.push(0xf5),
        Value::Unsigned(n) => write_uint(out, 0, *n),
        Value::Negative(n) => {
            if *n >= 0 {
                return Err(Error::dagcbor("negative variant holds non-negative"));
            }
            // CBOR negative: -1 - n encoded as unsigned
            let encoded = -1_i128 - i128::from(*n);
            write_uint(out, 1, encoded as u64);
        }
        Value::Bytes(b) => {
            write_uint(out, 2, b.len() as u64);
            out.extend_from_slice(b);
        }
        Value::Text(s) => write_text(out, s),
        Value::Array(items) => {
            write_uint(out, 4, items.len() as u64);
            for item in items {
                write_value(out, item)?;
            }
        }
        Value::Map(map) => write_map(out, map)?,
        Value::Cid(cid) => {
            // tag 42
            write_uint(out, 6, 42);
            let payload = cid.to_tagged_bytes();
            write_uint(out, 2, payload.len() as u64);
            out.extend_from_slice(&payload);
        }
    }
    Ok(())
}

fn write_text(out: &mut Vec<u8>, s: &str) {
    write_uint(out, 3, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn write_uint(out: &mut Vec<u8>, major: u8, n: u64) {
    let mt = major << 5;
    if n < 24 {
        out.push(mt | n as u8);
    } else if n <= u8::MAX as u64 {
        out.push(mt | 24);
        out.push(n as u8);
    } else if n <= u16::MAX as u64 {
        out.push(mt | 25);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= u32::MAX as u64 {
        out.push(mt | 26);
        out.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        out.push(mt | 27);
        out.extend_from_slice(&n.to_be_bytes());
    }
}

fn write_map(out: &mut Vec<u8>, map: &BTreeMap<String, Value>) -> Result<()> {
    let mut encoded_keys: Vec<(Vec<u8>, &str, &Value)> = Vec::with_capacity(map.len());
    for (k, v) in map {
        let mut key_cbor = Vec::new();
        write_text(&mut key_cbor, k);
        encoded_keys.push((key_cbor, k.as_str(), v));
    }
    // RFC 7049 §3.9: shorter encoded key first, then bytewise.
    encoded_keys.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
    write_uint(out, 5, encoded_keys.len() as u64);
    for (key_cbor, _, v) in encoded_keys {
        out.extend_from_slice(&key_cbor);
        write_value(out, v)?;
    }
    Ok(())
}

/// Convert `serde_json::Value` to DAG-CBOR [`Value`].
///
/// IPLD JSON CID links `{"/":"b…"}` become [`Value::Cid`]. Byte wrappers
/// `{"/":{"bytes":"<base64>"}}` become [`Value::Bytes`]. Other objects stay maps
/// (so vector CIDs stored as plain strings remain strings).
pub fn from_json(v: &serde_json::Value) -> Result<Value> {
    match v {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                Ok(Value::Unsigned(u))
            } else if let Some(i) = n.as_i64() {
                if i < 0 {
                    Ok(Value::Negative(i))
                } else {
                    Ok(Value::Unsigned(i as u64))
                }
            } else {
                Err(Error::dagcbor("floats are not allowed in DAG-CBOR hashing"))
            }
        }
        serde_json::Value::String(s) => Ok(Value::Text(s.clone())),
        serde_json::Value::Array(a) => {
            let mut items = Vec::with_capacity(a.len());
            for x in a {
                items.push(from_json(x)?);
            }
            Ok(Value::Array(items))
        }
        serde_json::Value::Object(m) => {
            if let Some(cid) = ipld_cid_link(m) {
                return Ok(Value::Cid(cid?));
            }
            if let Some(bytes) = ipld_bytes(m) {
                return Ok(Value::Bytes(bytes?));
            }
            let mut map = BTreeMap::new();
            for (k, val) in m {
                map.insert(k.clone(), from_json(val)?);
            }
            Ok(Value::Map(map))
        }
    }
}

fn ipld_cid_link(m: &serde_json::Map<String, serde_json::Value>) -> Option<Result<Cid>> {
    if m.len() != 1 {
        return None;
    }
    match m.get("/") {
        Some(serde_json::Value::String(s)) if s.starts_with('b') => Some(Cid::parse(s)),
        _ => None,
    }
}

fn ipld_bytes(m: &serde_json::Map<String, serde_json::Value>) -> Option<Result<Vec<u8>>> {
    if m.len() != 1 {
        return None;
    }
    let inner = m.get("/")?.as_object()?;
    if inner.len() != 1 {
        return None;
    }
    let b64 = inner.get("bytes")?.as_str()?;
    use base64::Engine;
    Some(
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| Error::dagcbor(format!("base64 bytes: {e}"))),
    )
}

/// Decode a complete DAG-CBOR value.
pub fn decode(bytes: &[u8]) -> Result<Value> {
    let (v, rest) = read_value(bytes)?;
    if !rest.is_empty() {
        return Err(Error::dagcbor("trailing bytes after DAG-CBOR value"));
    }
    Ok(v)
}

fn read_value(input: &[u8]) -> Result<(Value, &[u8])> {
    let (first, rest) = input
        .split_first()
        .ok_or_else(|| Error::dagcbor("unexpected end"))?;
    let major = first >> 5;
    let addl = first & 0x1f;
    let (n, rest) = read_addl(addl, rest)?;
    match major {
        0 => Ok((Value::Unsigned(n), rest)),
        1 => {
            let v = -1 - (n as i128);
            if v < i64::MIN as i128 {
                return Err(Error::dagcbor("negative integer out of i64"));
            }
            Ok((Value::Negative(v as i64), rest))
        }
        2 => {
            let len = usize_len(n)?;
            let (b, rest) = split_at_checked(rest, len)?;
            Ok((Value::Bytes(b.to_vec()), rest))
        }
        3 => {
            let len = usize_len(n)?;
            let (b, rest) = split_at_checked(rest, len)?;
            let s = std::str::from_utf8(b).map_err(|e| Error::dagcbor(format!("utf8: {e}")))?;
            Ok((Value::Text(s.to_string()), rest))
        }
        4 => {
            let len = usize_len(n)?;
            let mut items = Vec::with_capacity(len);
            let mut cur = rest;
            for _ in 0..len {
                let (v, next) = read_value(cur)?;
                items.push(v);
                cur = next;
            }
            Ok((Value::Array(items), cur))
        }
        5 => {
            let len = usize_len(n)?;
            let mut map = BTreeMap::new();
            let mut cur = rest;
            for _ in 0..len {
                let (k, next) = read_value(cur)?;
                let key = match k {
                    Value::Text(s) => s,
                    _ => return Err(Error::dagcbor("DAG-CBOR map keys must be text")),
                };
                let (v, next) = read_value(next)?;
                map.insert(key, v);
                cur = next;
            }
            Ok((Value::Map(map), cur))
        }
        6 => {
            if n != 42 {
                return Err(Error::dagcbor(format!("unsupported CBOR tag {n}")));
            }
            let (inner, rest) = read_value(rest)?;
            match inner {
                Value::Bytes(b) => Ok((Value::Cid(Cid::from_tagged_bytes(&b)?), rest)),
                _ => Err(Error::dagcbor("tag 42 must wrap bytes")),
            }
        }
        7 => match addl {
            20 => Ok((Value::Bool(false), rest)),
            21 => Ok((Value::Bool(true), rest)),
            22 => Ok((Value::Null, rest)),
            _ => Err(Error::dagcbor(format!(
                "unsupported major-7 additional {addl}"
            ))),
        },
        _ => Err(Error::dagcbor("invalid major type")),
    }
}

fn read_addl(addl: u8, rest: &[u8]) -> Result<(u64, &[u8])> {
    match addl {
        0..=23 => Ok((u64::from(addl), rest)),
        24 => {
            let (b, rest) = rest
                .split_first()
                .ok_or_else(|| Error::dagcbor("short u8"))?;
            Ok((u64::from(*b), rest))
        }
        25 => {
            let (b, rest) = split_at_checked(rest, 2)?;
            Ok((u64::from(u16::from_be_bytes([b[0], b[1]])), rest))
        }
        26 => {
            let (b, rest) = split_at_checked(rest, 4)?;
            Ok((
                u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])),
                rest,
            ))
        }
        27 => {
            let (b, rest) = split_at_checked(rest, 8)?;
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            Ok((u64::from_be_bytes(a), rest))
        }
        31 => Err(Error::dagcbor("indefinite-length items are forbidden")),
        _ => Err(Error::dagcbor("reserved additional info")),
    }
}

fn split_at_checked(input: &[u8], n: usize) -> Result<(&[u8], &[u8])> {
    if input.len() < n {
        return Err(Error::dagcbor("unexpected end of CBOR"));
    }
    Ok(input.split_at(n))
}

fn usize_len(n: u64) -> Result<usize> {
    usize::try_from(n).map_err(|_| Error::dagcbor("length exceeds usize"))
}

/// Encode a JSON object (vector-style, CID strings stay strings).
pub fn encode_json(v: &serde_json::Value) -> Result<Vec<u8>> {
    encode(&from_json(v)?)
}

/// JSON mapping of a DAG-CBOR value (IPLD JSON for bytes/CIDs).
pub fn to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Unsigned(n) => serde_json::json!(n),
        Value::Negative(n) => serde_json::json!(n),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(b);
            serde_json::json!({"/":{"bytes": b64}})
        }
        Value::Array(a) => serde_json::Value::Array(a.iter().map(to_json).collect()),
        Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in m {
                obj.insert(k.clone(), to_json(val));
            }
            serde_json::Value::Object(obj)
        }
        Value::Cid(c) => serde_json::json!({"/": c.to_string()}),
    }
}

pub fn decode_json(bytes: &[u8]) -> Result<serde_json::Value> {
    Ok(to_json(&decode(bytes)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_keys_length_then_lex() {
        let mut m = BTreeMap::new();
        m.insert("collection".into(), Value::Text("x".into()));
        m.insert("v".into(), Value::Unsigned(1));
        m.insert("ts".into(), Value::Unsigned(2));
        let bytes = encode(&Value::Map(m)).unwrap();
        // "v" (1), "ts" (2), then longer keys
        assert_eq!(bytes[0], 0xa3);
        assert_eq!(&bytes[1..3], b"\x61v");
    }
}
