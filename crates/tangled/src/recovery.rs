//! Offline verified backups of the KnotEngine persistence boundary.
//!
//! Local administrator API: archives contain private data and metadata. Checksums
//! detect damage, not a malicious archive author. Keep provenance trusted. Signing
//! and decryption keys, sessions, and external importer stores are not included.
//! Hold the engine lock for capture; stop the host before using the CLI.
use crate::{dagcbor, Cid, Error, KnotEngine, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

fn error(message: impl ToString) -> Error {
    Error::Other(format!("recovery: {}", message.to_string()))
}
fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    bytes: Vec<u8>,
    hash: String,
}

/// Versioned, checksummed inventory of engine persistence files.
#[derive(Clone, Serialize, Deserialize)]
pub struct Backup {
    version: u32,
    files: BTreeMap<String, Entry>,
    manifest_hash: String,
}

/// Destination work only. Source validation performs additional reads and writes.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScrubReport {
    pub source_files_verified: usize,
    pub source_payload_bytes_verified: usize,
    pub post_repair_files_read: usize,
    pub post_repair_bytes_read: usize,
    pub expected_files: usize,
    pub expected_bytes: usize,
    pub files_read: usize,
    pub bytes_read: usize,
    pub missing: Vec<String>,
    pub corrupt: Vec<String>,
    pub unexpected: Vec<String>,
    pub possible_newer_state: Vec<String>,
    pub repaired_files: usize,
    pub repaired_bytes: usize,
}
impl ScrubReport {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.corrupt.is_empty() && self.unexpected.is_empty()
    }
}

impl Backup {
    /// Capture all persistent engine state, including every historical block.
    /// The caller must hold the engine's write lock for a consistent snapshot.
    pub fn capture(engine: &KnotEngine) -> Result<Self> {
        validate_engine(engine)?;
        let stage = Stage::new()?;
        let mut snapshot = engine.clone();
        snapshot.data_dir = Some(stage.0.clone());
        snapshot.persist()?;
        let files = read_tree(&stage.0)?
            .into_iter()
            .map(|(path, bytes)| {
                let digest = hash(&bytes);
                (
                    path,
                    Entry {
                        bytes,
                        hash: digest,
                    },
                )
            })
            .collect();
        let mut backup = Self {
            version: 1,
            files,
            manifest_hash: String::new(),
        };
        backup.manifest_hash = backup.digest()?;
        backup.verify()?;
        Ok(backup)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(error)
    }

    /// Validate byte integrity and the full local dependency closure.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let backup: Self = serde_json::from_slice(bytes).map_err(error)?;
        backup.verify()?;
        Ok(backup)
    }

    fn digest(&self) -> Result<String> {
        Ok(hash(
            &serde_json::to_vec(&(self.version, &self.files)).map_err(error)?,
        ))
    }

    fn verify(&self) -> Result<()> {
        if self.version != 1 || self.digest()? != self.manifest_hash {
            return Err(error("manifest checksum/version mismatch"));
        }
        for required in [
            "describe.json",
            "config.json",
            "identities.json",
            "heads.json",
            "rev.json",
            "cap-uses.json",
            "subscribe.json",
            "blob-types.json",
            "revoked.json",
            "erased-cids.json",
        ] {
            if !self.files.contains_key(required) {
                return Err(error(format!("missing snapshot metadata: {required}")));
            }
        }
        for (path, entry) in &self.files {
            safe_path(path)?;
            if hash(&entry.bytes) != entry.hash {
                return Err(error(format!("file checksum mismatch: {path}")));
            }
        }
        let stage = Stage::new()?;
        self.write_new(&stage.0)?;
        let engine = KnotEngine::open(&stage.0, "did:sol:recovery-check")?;
        for cid in &engine.private_archive_erasure_cids {
            if self.files.contains_key(&format!("blocks/{cid}")) {
                return Err(error("archive contains erased block"));
            }
        }
        validate_engine(&engine)
    }

    fn write_new(&self, destination: &Path) -> Result<()> {
        for (path, entry) in &self.files {
            let target = destination.join(safe_path(path)?);
            private_dir_all(target.parent().ok_or_else(|| error("no parent"))?)?;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(target).map_err(error)?;
            file.write_all(&entry.bytes).map_err(error)?;
            file.sync_all().map_err(error)?;
        }
        Ok(())
    }

    /// Restore to an empty directory, then reopen the production engine.
    pub fn restore(&self, destination: &Path, did: &str) -> Result<KnotEngine> {
        self.verify()?;
        if destination.exists() {
            if fs::symlink_metadata(destination)
                .map_err(error)?
                .file_type()
                .is_symlink()
                || fs::read_dir(destination).map_err(error)?.next().is_some()
            {
                return Err(error("restore destination must be an empty directory"));
            }
        } else {
            private_dir_all(destination)?;
        }
        self.write_new(destination)?;
        KnotEngine::open(destination, did)
    }

    /// Compare the complete file inventory against a verified backup.
    pub fn scrub(&self, destination: &Path) -> Result<ScrubReport> {
        self.verify()?;
        self.compare(destination)
    }

    fn compare(&self, destination: &Path) -> Result<ScrubReport> {
        let current = read_tree(destination)?;
        let mut report = ScrubReport {
            source_files_verified: self.files.len(),
            source_payload_bytes_verified: self.files.values().map(|e| e.bytes.len()).sum(),
            expected_files: self.files.len(),
            expected_bytes: self.files.values().map(|e| e.bytes.len()).sum(),
            files_read: current.len(),
            bytes_read: current.values().map(Vec::len).sum(),
            ..Default::default()
        };
        for (path, entry) in &self.files {
            match current.get(path) {
                None => report.missing.push(path.clone()),
                Some(bytes) if bytes != &entry.bytes => {
                    report.corrupt.push(path.clone());
                    let valid_metadata = !path.contains('/')
                        && path.ends_with(".json")
                        && serde_json::from_slice::<serde_json::Value>(bytes).is_ok();
                    let valid_tip = path.ends_with("/tip")
                        && std::str::from_utf8(bytes)
                            .ok()
                            .is_some_and(|s| Cid::parse(s.trim()).is_ok());
                    if valid_metadata || valid_tip {
                        report.possible_newer_state.push(path.clone());
                    }
                }
                _ => (),
            }
        }
        report.unexpected = current
            .keys()
            .filter(|p| !self.files.contains_key(*p))
            .cloned()
            .collect();
        Ok(report)
    }

    /// Repair from a fully verified trusted source. Refuse extra files and newer
    /// erasure intent or valid changed mutable metadata instead of mixing snapshots.
    /// Lost mutable files still require a trusted matching snapshot. Replacements
    /// are atomic per file; this does not prove monotonic recovery after total loss.
    pub fn repair(&self, destination: &Path) -> Result<ScrubReport> {
        self.verify()?;
        let mut report = self.compare(destination)?;
        if !report.unexpected.is_empty() {
            return Err(error(
                "unexpected destination files; repair requires the matching snapshot",
            ));
        }
        if report
            .corrupt
            .iter()
            .any(|p| p == "private-archive-erasures.json" || p == "erased-cids.json")
        {
            return Err(error(
                "destination erasure intent differs; refusing rollback",
            ));
        }
        if !report.possible_newer_state.is_empty() {
            return Err(error(
                "valid mutable metadata differs; refusing possible state rollback",
            ));
        }
        for path in report.missing.iter().chain(&report.corrupt) {
            let target = destination.join(safe_path(path)?);
            let parent = target.parent().ok_or_else(|| error("no parent"))?;
            private_dir_all(parent)?;
            let pending = parent.join(format!(".recovery-{}", rand::random::<u64>()));
            let entry = &self.files[path];
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&pending).map_err(error)?;
            file.write_all(&entry.bytes).map_err(error)?;
            file.sync_all().map_err(error)?;
            fs::rename(&pending, &target).map_err(error)?;
            fs::File::open(parent)
                .and_then(|f| f.sync_all())
                .map_err(error)?;
            report.repaired_files += 1;
            report.repaired_bytes += entry.bytes.len();
        }
        let post_repair = self.compare(destination)?;
        report.post_repair_files_read = post_repair.files_read;
        report.post_repair_bytes_read = post_repair.bytes_read;
        if !post_repair.is_clean() {
            return Err(error("post-repair verification failed"));
        }
        Ok(report)
    }
}

fn private_dir_all(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(error)
}

fn safe_path(path: &str) -> Result<&Path> {
    let p = Path::new(path);
    if path.is_empty()
        || path.contains('\\')
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(error("unsafe archive path"));
    }
    Ok(p)
}

fn read_tree(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(root: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        let meta = fs::symlink_metadata(path).map_err(error)?;
        if meta.is_dir() {
            for entry in fs::read_dir(path).map_err(error)? {
                walk(root, &entry.map_err(error)?.path(), files)?;
            }
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(error)?
                .to_str()
                .ok_or_else(|| error("non-UTF8 path"))?
                .to_string();
            files.insert(rel, fs::read(path).map_err(error)?);
        } else {
            return Err(error("symlinks and special files are not supported"));
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files)?;
    Ok(files)
}

struct Stage(PathBuf);
impl Stage {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "tangled-recovery-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(error)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn links(value: &dagcbor::Value, out: &mut BTreeSet<String>) {
    match value {
        dagcbor::Value::Cid(cid) => {
            out.insert(cid.to_string());
        }
        dagcbor::Value::Text(text) => {
            if Cid::parse(text).is_ok() {
                out.insert(text.clone());
            }
        }
        dagcbor::Value::Array(values) => {
            for v in values {
                links(v, out);
            }
        }
        dagcbor::Value::Map(values) => {
            for v in values.values() {
                links(v, out);
            }
        }
        _ => (),
    }
}

fn validate_engine(engine: &KnotEngine) -> Result<()> {
    let mut available = BTreeSet::new();
    for (cid, bytes) in &engine.blocks {
        let parsed = Cid::parse(cid)?;
        let actual = if parsed.is_raw() {
            Cid::raw(bytes)
        } else {
            Cid::dag_cbor(bytes)
        };
        if parsed != actual {
            return Err(error(format!("corrupt block {cid}")));
        }
        available.insert(cid.clone());
    }
    for ((_, cid), bytes) in &engine.blobs {
        if Cid::parse(cid)? != Cid::raw(bytes) {
            return Err(error(format!("corrupt blob {cid}")));
        }
        available.insert(cid.clone());
    }
    if available
        .iter()
        .any(|cid| engine.private_archive_erasure_cids.contains(cid))
    {
        return Err(error("erased CID still present"));
    }
    let mut required = BTreeSet::new();
    for head in engine.heads.values() {
        let commit = engine
            .blocks
            .get(&head.commit)
            .ok_or_else(|| error("missing head commit"))?;
        let info = crate::record::parse_commit(commit)?;
        if info.did != head.did || info.data != head.data || info.rev != head.rev {
            return Err(error("head metadata disagrees with commit"));
        }
        let mut indexed = crate::Mst::new();
        for ((did, collection, rkey), cid) in &engine.tips {
            if did == &head.did {
                indexed.insert(
                    crate::sync::mst_key(collection, rkey).as_bytes(),
                    Cid::parse(cid)?,
                )?;
            }
        }
        if indexed.root_cid_string_cids()?.to_string() != head.data {
            return Err(error("record tips disagree with the committed MST root"));
        }
        required.insert(head.commit.clone());
        required.insert(head.data.clone());
    }
    let record_index: BTreeSet<_> = engine
        .stored
        .iter()
        .map(|r| {
            (
                r.did.as_str(),
                r.collection.as_str(),
                r.rkey.as_str(),
                r.cid.as_str(),
            )
        })
        .collect();
    for ((did, collection, rkey), cid) in &engine.tips {
        if !record_index.contains(&(
            did.as_str(),
            collection.as_str(),
            rkey.as_str(),
            cid.as_str(),
        )) {
            return Err(error("record tip has no matching persistent record"));
        }
    }
    required.extend(engine.tips.values().cloned());
    required.extend(engine.stored.iter().map(|r| r.cid.clone()));
    for (cid, bytes) in &engine.blocks {
        if !Cid::parse(cid)?.is_raw() {
            // Legacy unsigned test records use JSON; signed records, commits and
            // MST nodes use canonical DAG-CBOR.
            let value = dagcbor::decode(bytes).or_else(|_| {
                serde_json::from_slice(bytes)
                    .map_err(error)
                    .and_then(|v| dagcbor::from_json(&v))
            })?;
            links(&value, &mut required);
        }
    }
    for record in &engine.stored {
        let (cid, bytes) = crate::knot::record_cbor_and_cid(&record.record);
        if cid != record.cid
            || engine.blocks.get(&cid) != Some(&bytes)
            || record.record["author"].as_str() != Some(&record.did)
            || record.record["collection"].as_str() != Some(&record.collection)
            || record.record["rkey"].as_str() != Some(&record.rkey)
            || record.record["vis"].as_str() != Some(&record.vis)
            || record.record["body"].is_null() != record.deleted
            || record.record["circle"].as_str() != record.circle.as_deref()
        {
            return Err(error("record metadata disagrees with block"));
        }
        let mut refs = BTreeSet::new();
        links(&dagcbor::from_json(&record.record)?, &mut refs);
        for cid in refs {
            if Cid::parse(&cid)?.is_raw()
                && !engine
                    .blobs
                    .contains_key(&(record.did.clone(), cid.clone()))
                && !engine.private_archive_erasure_cids.contains(&cid)
            {
                return Err(error(format!("missing owner blob {cid}")));
            }
            required.insert(cid);
        }
    }
    for cid in required {
        if !available.contains(&cid) && !engine.private_archive_erasure_cids.contains(&cid) {
            return Err(error(format!("missing dependency {cid}")));
        }
    }
    Ok(())
}
