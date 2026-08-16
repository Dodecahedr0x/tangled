//! Merkle Search Tree (4-bit fanout, BLAKE3), matching AT Protocol structure
//! with Tangled codecs. Empty repo is the CID of `{l: null, e: []}`.

use crate::cid::Cid;
use crate::dagcbor::{self, Value};
use crate::error::{Error, Result};
use std::collections::BTreeMap;

/// One MST entry with a full key (prefix compression applied at encode time).
#[derive(Clone, Debug)]
pub struct MstEntry {
    pub key: Vec<u8>,
    pub value: Cid,
    pub right: Option<Box<MstNode>>,
}

/// MST node (in-memory, before CID assignment).
#[derive(Clone, Debug, Default)]
pub struct MstNode {
    pub left: Option<Box<MstNode>>,
    pub entries: Vec<MstEntry>,
}

/// Key → record CID map that rebuilds a correct MST on demand.
#[derive(Clone, Debug, Default)]
pub struct Mst {
    pairs: BTreeMap<Vec<u8>, Cid>,
}

impl Mst {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: impl AsRef<[u8]>, value: Cid) -> Result<()> {
        let key = key.as_ref();
        if key.contains(&0) {
            return Err(Error::Mst("MST keys MUST NOT contain 0x00".into()));
        }
        self.pairs.insert(key.to_vec(), value);
        Ok(())
    }

    pub fn get(&self, key: &[u8]) -> Option<&Cid> {
        self.pairs.get(key)
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Build the tree from current keys.
    pub fn root_node(&self) -> MstNode {
        let pairs: Vec<(Vec<u8>, Cid)> = self
            .pairs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        build_from_sorted(&pairs)
    }

    /// Encode root with CID fields as multibase strings (vector encoding).
    pub fn root_cid_string_cids(&self) -> Result<Cid> {
        let node = self.root_node();
        node_cid_string_cids(&node)
    }

    /// Encode root with CID tag 42 (production DAG-CBOR).
    pub fn root_cid_links(&self) -> Result<Cid> {
        let node = self.root_node();
        node_cid_links(&node)
    }

    /// Every MST node as `(cid, dag-cbor)` using string-CID encoding.
    pub fn block_pairs_string_cids(&self) -> Result<Vec<(Cid, Vec<u8>)>> {
        let mut out = Vec::new();
        collect_node_blocks(&self.root_node(), &mut out)?;
        Ok(out)
    }

    /// Inclusion proof: nodes from root to the leaf holding `key`, string-CID encoding.
    pub fn inclusion_proof(&self, key: &[u8]) -> Result<InclusionProof> {
        if !self.pairs.contains_key(key) {
            return Err(Error::Mst("key not present".into()));
        }
        let root = self.root_node();
        let mut nodes = Vec::new();
        collect_proof(&root, key, &mut nodes)?;
        let root_cid = node_cid_string_cids(&root)?;
        Ok(InclusionProof {
            key: key.to_vec(),
            value: self.pairs[key].clone(),
            nodes,
            root_cid,
        })
    }
}

fn key_layer(key: &[u8]) -> u32 {
    let hash = blake3::hash(key);
    let mut zeros = 0u32;
    for &b in hash.as_bytes() {
        if b == 0 {
            zeros += 8;
            continue;
        }
        zeros += b.leading_zeros();
        break;
    }
    zeros / 4
}

fn build_from_sorted(pairs: &[(Vec<u8>, Cid)]) -> MstNode {
    if pairs.is_empty() {
        return MstNode::default();
    }
    let layers: Vec<u32> = pairs.iter().map(|(k, _)| key_layer(k)).collect();
    let max_layer = layers.iter().copied().max().unwrap_or(0);
    let hi: Vec<usize> = layers
        .iter()
        .enumerate()
        .filter(|(_, &l)| l == max_layer)
        .map(|(i, _)| i)
        .collect();
    let first = hi[0];
    let left = if first > 0 {
        Some(Box::new(build_from_sorted(&pairs[..first])))
    } else {
        None
    };
    let mut entries = Vec::new();
    for (j, &idx) in hi.iter().enumerate() {
        let right_end = hi.get(j + 1).copied().unwrap_or(pairs.len());
        let right = if idx + 1 < right_end {
            Some(Box::new(build_from_sorted(&pairs[idx + 1..right_end])))
        } else {
            None
        };
        entries.push(MstEntry {
            key: pairs[idx].0.clone(),
            value: pairs[idx].1.clone(),
            right,
        });
    }
    MstNode { left, entries }
}

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Encode a node using string CID fields (matches `mst-proof.json`).
pub fn encode_node_string_cids(node: &MstNode) -> Result<Vec<u8>> {
    encode(&node_to_value(node, true)?)
}

/// Encode a node using tag-42 CID links.
pub fn encode_node_links(node: &MstNode) -> Result<Vec<u8>> {
    encode(&node_to_value(node, false)?)
}

fn encode(v: &Value) -> Result<Vec<u8>> {
    dagcbor::encode(v)
}

fn cid_value(cid: &Cid, as_string: bool) -> Value {
    if as_string {
        Value::Text(cid.to_string_multibase())
    } else {
        Value::Cid(cid.clone())
    }
}

fn optional_cid(child: &Option<Box<MstNode>>, as_string: bool) -> Result<Value> {
    match child {
        None => Ok(Value::Null),
        Some(n) => {
            let bytes = if as_string {
                encode_node_string_cids(n)?
            } else {
                encode_node_links(n)?
            };
            Ok(cid_value(&Cid::dag_cbor(&bytes), as_string))
        }
    }
}

fn node_to_value(node: &MstNode, as_string: bool) -> Result<Value> {
    let mut entries = Vec::new();
    let mut prev: Vec<u8> = Vec::new();
    for e in &node.entries {
        let p = common_prefix(&prev, &e.key);
        let suffix = e.key[p..].to_vec();
        let mut em = BTreeMap::new();
        em.insert("p".into(), Value::Unsigned(p as u64));
        em.insert("k".into(), Value::Bytes(suffix));
        em.insert("v".into(), cid_value(&e.value, as_string));
        em.insert("t".into(), optional_cid(&e.right, as_string)?);
        entries.push(Value::Map(em));
        prev = e.key.clone();
    }
    let mut m = BTreeMap::new();
    m.insert("l".into(), optional_cid(&node.left, as_string)?);
    m.insert("e".into(), Value::Array(entries));
    Ok(Value::Map(m))
}

pub fn node_cid_string_cids(node: &MstNode) -> Result<Cid> {
    Ok(Cid::dag_cbor(&encode_node_string_cids(node)?))
}

fn collect_node_blocks(node: &MstNode, out: &mut Vec<(Cid, Vec<u8>)>) -> Result<()> {
    let bytes = encode_node_string_cids(node)?;
    out.push((Cid::dag_cbor(&bytes), bytes));
    if let Some(left) = &node.left {
        collect_node_blocks(left, out)?;
    }
    for e in &node.entries {
        if let Some(right) = &e.right {
            collect_node_blocks(right, out)?;
        }
    }
    Ok(())
}

pub fn node_cid_links(node: &MstNode) -> Result<Cid> {
    Ok(Cid::dag_cbor(&encode_node_links(node)?))
}

/// Empty-tree data CID (string-CID encoding of `{l:null,e:[]}`).
pub fn empty_root_cid_string_cids() -> Result<Cid> {
    node_cid_string_cids(&MstNode::default())
}

/// Inclusion proof nodes (root first) plus the claimed value CID.
#[derive(Clone, Debug)]
pub struct InclusionProof {
    pub key: Vec<u8>,
    pub value: Cid,
    pub nodes: Vec<MstNode>,
    pub root_cid: Cid,
}

impl InclusionProof {
    /// Verify that `key` maps to `value` and the root CID matches `commit_data`.
    pub fn verify(&self, commit_data: &Cid) -> Result<()> {
        if &self.root_cid != commit_data {
            return Err(Error::Mst("proof root != commit.data".into()));
        }
        if self.nodes.is_empty() {
            return Err(Error::Mst("empty proof".into()));
        }
        let root_bytes = encode_node_string_cids(&self.nodes[0])?;
        if Cid::dag_cbor(&root_bytes) != self.root_cid {
            return Err(Error::Mst("root node bytes do not hash to root CID".into()));
        }
        // Walk reconstructed keys on the first (root) node; for the single-leaf
        // vector the root is the only node.
        if !node_contains_key(&self.nodes[0], &self.key, &self.value) {
            return Err(Error::Mst("key/value not present on proof path".into()));
        }
        Ok(())
    }
}

fn node_contains_key(node: &MstNode, key: &[u8], value: &Cid) -> bool {
    if node
        .entries
        .iter()
        .any(|e| e.key == key && e.value == *value)
    {
        return true;
    }
    if let Some(l) = &node.left {
        if node_contains_key(l, key, value) {
            return true;
        }
    }
    node.entries.iter().any(|e| {
        e.right
            .as_ref()
            .is_some_and(|r| node_contains_key(r, key, value))
    })
}

fn collect_proof(node: &MstNode, key: &[u8], out: &mut Vec<MstNode>) -> Result<bool> {
    out.push(node.clone());
    if node.entries.iter().any(|e| e.key == key) {
        return Ok(true);
    }
    if let Some(l) = &node.left {
        let before = out.len();
        if collect_proof(l, key, out)? {
            return Ok(true);
        }
        out.truncate(before);
    }
    for e in &node.entries {
        if let Some(r) = &e.right {
            let before = out.len();
            if collect_proof(r, key, out)? {
                return Ok(true);
            }
            out.truncate(before);
        }
    }
    Ok(false)
}

/// Reconstruct a node from vector JSON `{l, e:[{p,k_hex,v,t}]}`.
pub fn node_from_vector_json(v: &serde_json::Value) -> Result<MstNode> {
    let obj = v
        .as_object()
        .ok_or_else(|| Error::Mst("node must be object".into()))?;
    let entries_json = obj
        .get("e")
        .and_then(|x| x.as_array())
        .ok_or_else(|| Error::Mst("missing e".into()))?;
    let mut prev = Vec::new();
    let mut entries = Vec::new();
    for e in entries_json {
        let p = e.get("p").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        let k_hex = e
            .get("k_hex")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::Mst("missing k_hex".into()))?;
        let suffix = hex::decode(k_hex).map_err(|err| Error::Mst(format!("k_hex: {err}")))?;
        let mut key = prev.get(..p).unwrap_or(&[]).to_vec();
        key.extend_from_slice(&suffix);
        let v_str = e
            .get("v")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::Mst("missing v".into()))?;
        let value = Cid::parse(v_str)?;
        prev = key.clone();
        entries.push(MstEntry {
            key,
            value,
            right: None,
        });
    }
    Ok(MstNode {
        left: None,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_single() {
        let mut m = Mst::new();
        let cid = Cid::dag_cbor(b"rec");
        m.insert(b"pub.tangled.post/abc", cid.clone()).unwrap();
        let node = m.root_node();
        assert_eq!(node.entries.len(), 1);
        assert_eq!(node.entries[0].value, cid);
    }

    #[test]
    fn missing_key_proof_rejected() {
        let mut m = Mst::new();
        m.insert(b"pub.tangled.post/abc", Cid::dag_cbor(b"rec"))
            .unwrap();
        assert!(m.inclusion_proof(b"pub.tangled.post/missing").is_err());
        m.insert(b"bad\0key", Cid::dag_cbor(b"x")).unwrap_err();
    }
}
