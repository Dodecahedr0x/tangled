//! CARv1 encode/decode for repository export (spec 07 / 11).
//!
//! Header `roots` is the commit CID. Each block is `varint(len(CID||data)) || CID || data`.

use crate::cid::Cid;
use crate::dagcbor::{self, Value};
use crate::error::{Error, Result};
use std::collections::BTreeMap;

/// One CARv1 archive.
#[derive(Clone, Debug, Default)]
pub struct Car {
    pub roots: Vec<Cid>,
    pub blocks: Vec<(Cid, Vec<u8>)>,
}

impl Car {
    pub fn get(&self, cid: &Cid) -> Option<&[u8]> {
        self.blocks
            .iter()
            .find(|(c, _)| c == cid)
            .map(|(_, b)| b.as_slice())
    }

    pub fn block_cids(&self) -> Vec<String> {
        self.blocks.iter().map(|(c, _)| c.to_string()).collect()
    }
}

/// Encode a CARv1 byte string.
pub fn encode(car: &Car) -> Result<Vec<u8>> {
    let header = header_value(&car.roots)?;
    let header_bytes = dagcbor::encode(&header)?;
    let mut out = Vec::new();
    write_uvarint(&mut out, header_bytes.len() as u64);
    out.extend_from_slice(&header_bytes);
    for (cid, data) in &car.blocks {
        let cid_bytes = cid.to_bytes();
        let len = cid_bytes.len() + data.len();
        write_uvarint(&mut out, len as u64);
        out.extend_from_slice(&cid_bytes);
        out.extend_from_slice(data);
    }
    Ok(out)
}

/// Decode a CARv1 byte string.
pub fn decode(bytes: &[u8]) -> Result<Car> {
    if bytes.is_empty() {
        return Err(Error::Codec("empty CAR".into()));
    }
    let (hlen, rest) = read_uvarint(bytes)?;
    let hlen = usize::try_from(hlen).map_err(|_| Error::Codec("CAR header length".into()))?;
    if rest.len() < hlen {
        return Err(Error::Codec("truncated CAR header".into()));
    }
    let (header_bytes, mut rest) = rest.split_at(hlen);
    let header = dagcbor::decode(header_bytes)?;
    let roots = parse_roots(&header)?;
    let mut blocks = Vec::new();
    while !rest.is_empty() {
        let (blen, after) = read_uvarint(rest)?;
        let blen = usize::try_from(blen).map_err(|_| Error::Codec("CAR block length".into()))?;
        if after.len() < blen {
            return Err(Error::Codec("truncated CAR block".into()));
        }
        let (block, after) = after.split_at(blen);
        rest = after;
        if block.len() < 36 {
            return Err(Error::Codec("CAR block shorter than CIDv1".into()));
        }
        let cid = Cid::from_bytes(&block[..36])?;
        let data = block[36..].to_vec();
        if cid.is_dag_cbor() && Cid::dag_cbor(&data) != cid {
            return Err(Error::Codec(format!("CAR block CID mismatch for {}", cid)));
        }
        if cid.is_raw() && Cid::raw(&data) != cid {
            return Err(Error::Codec(format!(
                "CAR raw block CID mismatch for {}",
                cid
            )));
        }
        blocks.push((cid, data));
    }
    Ok(Car { roots, blocks })
}

fn header_value(roots: &[Cid]) -> Result<Value> {
    let mut m = BTreeMap::new();
    m.insert("version".into(), Value::Unsigned(1));
    m.insert(
        "roots".into(),
        Value::Array(roots.iter().cloned().map(Value::Cid).collect()),
    );
    Ok(Value::Map(m))
}

fn parse_roots(header: &Value) -> Result<Vec<Cid>> {
    let version = header
        .get("version")
        .and_then(|v| match v {
            Value::Unsigned(n) => Some(*n),
            _ => None,
        })
        .ok_or_else(|| Error::Codec("CAR header missing version".into()))?;
    if version != 1 {
        return Err(Error::Codec(format!("unsupported CAR version {version}")));
    }
    let roots = header
        .get("roots")
        .ok_or_else(|| Error::Codec("CAR header missing roots".into()))?;
    match roots {
        Value::Array(items) => items
            .iter()
            .map(|v| match v {
                Value::Cid(c) => Ok(c.clone()),
                _ => Err(Error::Codec("CAR root must be a CID".into())),
            })
            .collect(),
        _ => Err(Error::Codec("CAR roots must be an array".into())),
    }
}

fn write_uvarint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn read_uvarint(input: &[u8]) -> Result<(u64, &[u8])> {
    let mut n = 0u64;
    let mut shift = 0;
    for (i, &b) in input.iter().enumerate() {
        if shift >= 64 {
            return Err(Error::Codec("uvarint overflow".into()));
        }
        n |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok((n, &input[i + 1..]));
        }
        shift += 7;
    }
    Err(Error::Codec("truncated uvarint".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_small_repo() {
        let rec = b"{\"v\":1}";
        let cid = Cid::dag_cbor(rec);
        let car = Car {
            roots: vec![cid.clone()],
            blocks: vec![(cid.clone(), rec.to_vec())],
        };
        let bytes = encode(&car).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back.roots, car.roots);
        assert_eq!(back.get(&cid), Some(rec.as_slice()));
    }

    #[test]
    fn rejects_empty() {
        assert!(decode(&[]).is_err());
    }
}
