//! getBlob authorization, sealed MST keys, backlink/query helpers (spec 08 / 11).

use crate::error::Result;
use crate::hpke;

/// One referencing record for getBlob policy.
#[derive(Clone, Debug)]
pub struct BlobRef {
    pub collection: String,
    pub rkey: String,
    pub vis: String,
    pub deleted: bool,
    pub references_blob: bool,
    pub circle: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlobAuth {
    ServeWithoutCap,
    RequireCap,
}

/// Serve without a capability iff a live `vis: public` record references the blob.
pub fn getblob_auth(refs: &[BlobRef]) -> BlobAuth {
    if refs
        .iter()
        .any(|r| r.vis == "public" && !r.deleted && r.references_blob)
    {
        BlobAuth::ServeWithoutCap
    } else {
        BlobAuth::RequireCap
    }
}

/// MST key for a public/`cap` collection.
pub fn mst_key(collection: &str, rkey: &str) -> String {
    format!("{collection}/{rkey}")
}

/// Encrypted rkey for `vis: sealed` collections (spec 07).
pub fn sealed_mst_key(collection: &str, epoch_secret: &[u8; 32], rkey: &str) -> Result<String> {
    let enc = hpke::encrypt_rkey(epoch_secret, rkey)?;
    Ok(format!("{collection}/{enc}"))
}

/// Safe DID rkey: `did:sol:X` → `did.sol.X`.
pub fn safe_did_rkey(did: &str) -> String {
    did.replace(':', ".")
}

/// Client cache of verified heads (spec 11). A lower `rev` is rollback.
#[derive(Clone, Debug, Default)]
pub struct HeadCache {
    inner: std::collections::HashMap<String, (String, String)>,
}

impl HeadCache {
    pub fn observe(&mut self, did: &str, commit: &str, rev: &str) -> Result<()> {
        if let Some((old_commit, old_rev)) = self.inner.get(did) {
            if tid_before(rev, old_rev) && commit != old_commit {
                return Err(crate::error::Error::Identity(format!(
                    "rollback: {did} rev {rev} < {old_rev}"
                )));
            }
        }
        self.inner
            .insert(did.to_string(), (commit.to_string(), rev.to_string()));
        Ok(())
    }

    pub fn get(&self, did: &str) -> Option<&(String, String)> {
        self.inner.get(did)
    }
}

/// TID / numeric rev comparison (spec 07 TID order).
pub fn tid_before(a: &str, b: &str) -> bool {
    if a.len() == b.len() {
        return a < b;
    }
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x < y,
        _ => a < b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_ref_serves() {
        let refs = [BlobRef {
            collection: "pub.tangled.media".into(),
            rkey: "a".into(),
            vis: "public".into(),
            deleted: false,
            references_blob: true,
            circle: None,
        }];
        assert_eq!(getblob_auth(&refs), BlobAuth::ServeWithoutCap);
    }

    #[test]
    fn sealed_requires() {
        let refs = [BlobRef {
            collection: "pub.tangled.media".into(),
            rkey: "a".into(),
            vis: "sealed".into(),
            deleted: false,
            references_blob: true,
            circle: None,
        }];
        assert_eq!(getblob_auth(&refs), BlobAuth::RequireCap);
    }

    #[test]
    fn sealed_mst_key_hides_plaintext_rkey() {
        let secret = [3u8; 32];
        let key = sealed_mst_key("pub.tangled.chat", &secret, "hello").unwrap();
        assert!(key.starts_with("pub.tangled.chat/"));
        assert!(!key.contains("hello"));
    }

    #[test]
    fn head_cache_detects_rollback() {
        let mut c = HeadCache::default();
        c.observe("did:sol:a", "c1", "3jzfcijpjiz2u").unwrap();
        assert!(c.observe("did:sol:a", "c0", "3jzfcijpjiz2t").is_err());
        c.observe("did:sol:a", "c2", "3jzfcijpjiz2v").unwrap();
        assert_eq!(c.get("did:sol:a").unwrap().0, "c2");
    }
}
