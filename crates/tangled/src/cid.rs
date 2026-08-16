//! CIDv1: codec dag-cbor (0x71) or raw (0x55), multihash blake3 (0x1e).
//! Display is multibase `b` (lowercase base32, no padding).

use std::fmt;

use crate::error::{Error, Result};

const MULTIBASE_BASE32: u8 = b'b';
const CIDV1: u8 = 0x01;
const CODEC_DAG_CBOR: u8 = 0x71;
const CODEC_RAW: u8 = 0x55;
const MH_BLAKE3: u8 = 0x1e;
const MH_LEN: u8 = 32;
const CID_RAW_LEN: usize = 4 + 32;
const BASE32_ALPH: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Content Identifier (CIDv1 + blake3-256).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Cid {
    codec: u8,
    digest: [u8; 32],
}

impl Cid {
    /// CIDv1 dag-cbor blake3 of `bytes`.
    pub fn dag_cbor(bytes: &[u8]) -> Self {
        Self {
            codec: CODEC_DAG_CBOR,
            digest: *blake3::hash(bytes).as_bytes(),
        }
    }

    /// CIDv1 raw blake3 of `bytes`.
    pub fn raw(bytes: &[u8]) -> Self {
        Self {
            codec: CODEC_RAW,
            digest: *blake3::hash(bytes).as_bytes(),
        }
    }

    /// Binary CIDv1 (no multibase prefix): `0x01 | codec | 0x1e | 0x20 | digest`.
    pub fn to_bytes(&self) -> [u8; CID_RAW_LEN] {
        let mut raw = [0u8; CID_RAW_LEN];
        raw[0] = CIDV1;
        raw[1] = self.codec;
        raw[2] = MH_BLAKE3;
        raw[3] = MH_LEN;
        raw[4..].copy_from_slice(&self.digest);
        raw
    }

    /// Multibase base32 (`b` + lowercase, no padding).
    pub fn to_string_multibase(&self) -> String {
        let raw = self.to_bytes();
        let mut out = String::with_capacity(1 + (CID_RAW_LEN * 8).div_ceil(5));
        out.push(MULTIBASE_BASE32 as char);
        out.push_str(&base32_encode(&raw));
        out
    }

    /// DAG-CBOR CID link payload: `0x00 || cid-bytes` (identity multibase).
    pub fn to_tagged_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + CID_RAW_LEN);
        v.push(0x00);
        v.extend_from_slice(&self.to_bytes());
        v
    }

    /// Parse a `b…` multibase CID, or raw 36-byte CIDv1.
    pub fn parse(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix('b') {
            let bytes = base32_decode(rest)?;
            return Self::from_bytes(&bytes);
        }
        Err(Error::InvalidCid(format!("expected multibase b…, got {s}")))
    }

    /// Parse raw CIDv1 bytes (no multibase).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != CID_RAW_LEN {
            return Err(Error::InvalidCid(format!(
                "CIDv1 blake3 must be {CID_RAW_LEN} bytes, got {}",
                bytes.len()
            )));
        }
        if bytes[0] != CIDV1 {
            return Err(Error::InvalidCid("not CIDv1".into()));
        }
        if bytes[2] != MH_BLAKE3 || bytes[3] != MH_LEN {
            return Err(Error::InvalidCid("expected blake3-256 multihash".into()));
        }
        let codec = bytes[1];
        if codec != CODEC_DAG_CBOR && codec != CODEC_RAW {
            return Err(Error::InvalidCid(format!(
                "unsupported codec 0x{codec:02x}"
            )));
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&bytes[4..]);
        Ok(Self { codec, digest })
    }

    /// Parse DAG-CBOR tag(42) payload (`0x00 || cidv1`).
    pub fn from_tagged_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.first() != Some(&0x00) {
            return Err(Error::InvalidCid(
                "CID tag 42 payload must start with 0x00".into(),
            ));
        }
        Self::from_bytes(&bytes[1..])
    }

    pub fn codec(&self) -> u8 {
        self.codec
    }

    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    pub fn is_dag_cbor(&self) -> bool {
        self.codec == CODEC_DAG_CBOR
    }

    pub fn is_raw(&self) -> bool {
        self.codec == CODEC_RAW
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_multibase())
    }
}

impl std::str::FromStr for Cid {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl PartialOrd for Cid {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cid {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string_multibase().cmp(&other.to_string_multibase())
    }
}

fn base32_encode(bytes: &[u8]) -> String {
    let mut bits: u32 = 0;
    let mut value: u32 = 0;
    let mut out = String::new();
    for &b in bytes {
        value = (value << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            out.push(BASE32_ALPH[((value >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(BASE32_ALPH[((value << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn base32_decode(s: &str) -> Result<Vec<u8>> {
    let mut bits: u32 = 0;
    let mut value: u32 = 0;
    let mut out = Vec::new();
    for c in s.chars() {
        let idx = BASE32_ALPH
            .iter()
            .position(|&a| a == c as u8)
            .ok_or_else(|| Error::InvalidCid(format!("invalid base32 char {c}")))?;
        value = (value << 5) | idx as u32;
        bits += 5;
        if bits >= 8 {
            out.push((value >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_display() {
        let c = Cid::dag_cbor(b"hello");
        let s = c.to_string_multibase();
        assert!(s.starts_with('b'));
        assert_eq!(Cid::parse(&s).unwrap(), c);
    }

    #[test]
    fn raw_codec() {
        let c = Cid::raw(b"blob");
        assert!(c.is_raw());
        assert_eq!(Cid::parse(&c.to_string()).unwrap(), c);
    }
}
