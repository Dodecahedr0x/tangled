//! Portable owner repository contents. Secret fields never enter command output.
use super::*;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub(crate) const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub(crate) struct BackupContents {
    pub version: u32,
    pub owner: String,
    ed25519: String,
    x25519: String,
    car: Option<String>,
    blobs: Vec<BackupBlob>,
}
#[derive(Serialize, Deserialize)]
struct BackupBlob {
    cid: String,
    data: String,
}
impl Drop for BackupContents {
    fn drop(&mut self) {
        self.ed25519.zeroize();
        self.x25519.zeroize();
        if let Some(car) = &mut self.car {
            car.zeroize();
        }
        for blob in &mut self.blobs {
            blob.data.zeroize();
        }
    }
}
impl Store {
    pub(crate) fn backup_contents(&mut self) -> Result<BackupContents> {
        let owner = self.did();
        let car = if self.engine.heads.contains_key(&owner) {
            Some(self.export()?)
        } else {
            None
        };
        let bytes = self
            .engine
            .blobs
            .iter()
            .filter(|((did, _), _)| did == &owner)
            .try_fold(car.as_ref().map_or(0, Vec::len), |total, (_, bytes)| {
                total.checked_add(bytes.len()).ok_or("Backup size overflow")
            })?;
        if bytes > (MAX_CONTENT_BYTES - 8192) / 2 {
            return Err("Backup contents exceed 64 MiB encoded limit".into());
        }
        let (ed, x) = self.identity.to_seeds();
        let ed = Zeroizing::new(ed);
        let x = Zeroizing::new(x);
        let mut blobs: Vec<_> = self
            .engine
            .blobs
            .iter()
            .filter(|((did, _), _)| did == &owner)
            .map(|((_, cid), bytes)| BackupBlob {
                cid: cid.clone(),
                data: hex::encode(bytes),
            })
            .collect();
        blobs.sort_by(|a, b| a.cid.cmp(&b.cid));
        Ok(BackupContents {
            version: 1,
            owner,
            ed25519: hex::encode(*ed),
            x25519: hex::encode(*x),
            car: car.map(hex::encode),
            blobs,
        })
    }

    pub(crate) fn verify_contents(contents: &BackupContents) -> Result<Self> {
        if contents.version != 1 {
            return Err("Unsupported backup contents".into());
        }
        let ed = Zeroizing::new(decode_seed(&contents.ed25519)?);
        let x = Zeroizing::new(decode_seed(&contents.x25519)?);
        let identity = Identity::from_seeds(*ed, *x);
        if identity.did_string() != contents.owner {
            return Err("Backup owner identity mismatch".into());
        }
        let mut staged = Self::memory();
        staged.engine = KnotEngine::hosting(&contents.owner);
        staged.engine.now = now();
        staged.engine.register_identity(&identity);
        staged.identity = identity;
        if let Some(car) = &contents.car {
            let bytes =
                Zeroizing::new(hex::decode(car).map_err(|_| "Invalid backup CAR encoding")?);
            staged.import(&bytes)?;
        }
        let mut seen = std::collections::HashSet::new();
        for blob in &contents.blobs {
            if !seen.insert(&blob.cid) {
                return Err("Duplicate backup attachment".into());
            }
            let bytes =
                hex::decode(&blob.data).map_err(|_| "Invalid backup attachment encoding")?;
            if Cid::raw(&bytes).to_string() != blob.cid {
                return Err("Backup attachment CID mismatch".into());
            }
            if bytes.len() > 32 * 1024 * 1024 {
                return Err("Backup attachment exceeds 32 MiB".into());
            }
            staged
                .engine
                .blobs
                .insert((contents.owner.clone(), blob.cid.clone()), bytes.clone());
            staged.engine.blocks.insert(blob.cid.clone(), bytes);
        }
        Ok(staged)
    }

    pub(crate) fn continuity_status(&self) -> Value {
        let owner = self.did();
        json!({"owner":owner,"head":self.engine.heads.get(&owner).map(|h| &h.commit),
            "records":self.engine.tips.keys().filter(|(did,_,_)| did == &owner).count(),
            "blobs":self.engine.blobs.keys().filter(|(did,_)| did == &owner).count()})
    }

    pub(crate) fn restore_contents(contents: &BackupContents, destination: &Path) -> Result<Value> {
        let mut staged = Self::verify_contents(contents)?;
        // create_dir is deliberately atomic and refuses any existing directory or symlink.
        std::fs::create_dir(destination).map_err(|_| {
            "Restore requires a fresh destination directory with an existing parent"
        })?;
        private_dir(destination)?;
        #[derive(Serialize)]
        struct IdentityFile<'a> {
            ed25519: &'a str,
            x25519: &'a str,
            did: &'a str,
        }
        let identity = Zeroizing::new(
            serde_json::to_vec(&IdentityFile {
                ed25519: &contents.ed25519,
                x25519: &contents.x25519,
                did: &contents.owner,
            })
            .map_err(err)?,
        );
        use std::io::Write;
        let mut file = private_file(&destination.join("identity.json"), true)?;
        file.write_all(&identity).map_err(err)?;
        file.sync_all().map_err(err)?;
        let mut target = Self::open(destination)?;
        staged.engine.data_dir = Some(destination.to_path_buf());
        staged.engine.persist().map_err(err)?;
        target.engine = staged.engine;
        Ok(target.continuity_status())
    }

    pub(crate) fn sync_contents(&mut self, contents: &BackupContents) -> Result<Value> {
        let verified = Self::verify_contents(contents)?;
        if contents.owner != self.did() {
            return Err("Transfer belongs to a different owner".into());
        }
        if contents.car.is_none() && self.engine.heads.contains_key(&self.did()) {
            return Err("Transfer would roll back the current repository".into());
        }
        // Stage all changes so rejection never alters the live repository.
        let mut candidate = Self::memory();
        let (ed, x) = self.identity.to_seeds();
        let ed = Zeroizing::new(ed);
        let x = Zeroizing::new(x);
        candidate.identity = Identity::from_seeds(*ed, *x);
        candidate.engine = self.engine.clone();
        candidate.engine.data_dir = None;
        if let Some(car) = &contents.car {
            let bytes =
                Zeroizing::new(hex::decode(car).map_err(|_| "Invalid backup CAR encoding")?);
            candidate.import(&bytes)?;
        }
        for ((did, cid), bytes) in verified.engine.blobs {
            candidate.engine.blocks.insert(cid.clone(), bytes.clone());
            candidate.engine.blobs.insert((did, cid), bytes);
        }
        candidate.engine.data_dir = self.path.clone();
        candidate.engine.persist().map_err(err)?;
        self.engine = candidate.engine;
        Ok(self.continuity_status())
    }
}

fn decode_seed(value: &str) -> Result<[u8; 32]> {
    let bytes = Zeroizing::new(hex::decode(value).map_err(|_| "Invalid backup owner seed")?);
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Backup owner seed must be 32 bytes".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_identity_and_attachment_corruption_before_restore() {
        let mut source = Store::memory();
        source.put_blob(b"private bytes", "text/plain").unwrap();
        let mut contents = source.backup_contents().unwrap();
        contents.blobs[0].data = hex::encode(b"different bytes");
        assert!(Store::verify_contents(&contents)
            .err()
            .unwrap()
            .contains("CID"));
        contents.blobs.clear();
        contents.car = Some(hex::encode(b"damaged signed repository"));
        assert!(Store::verify_contents(&contents).is_err());
        contents.car = None;
        contents.ed25519 = hex::encode([7; 32]);
        assert!(Store::verify_contents(&contents)
            .err()
            .unwrap()
            .contains("identity"));
    }
}
