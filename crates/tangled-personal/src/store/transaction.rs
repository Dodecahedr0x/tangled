//! Recoverable publication for Store imports. The owner lock and identity never move.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::Write;

const JOURNAL: &str = ".personal-transaction";
// Only engine snapshot entries move. Other owner files remain untouched.
const ENTRIES: &[&str] = &[
    "describe.json",
    "config.json",
    "identities.json",
    "heads.json",
    "erased-cids.json",
    "rev.json",
    "cap-uses.json",
    "subscribe.json",
    "blob-types.json",
    "revoked.json",
    "peers-state.json",
    "private-archive-erasures.json",
    "repos",
    "blobs",
    "blocks",
];

#[derive(Serialize, Deserialize)]
struct Manifest {
    original: Vec<String>,
    staged: Vec<String>,
}

impl Store {
    pub(super) fn publish_candidate(&mut self, mut candidate: KnotEngine) -> Result<()> {
        if let Some(path) = &self.path {
            recover(path)?;
            prepare(path, &mut candidate)?;
            if let Err(error) = publish(path) {
                return match recover(path) {
                    Ok(()) => Err(error),
                    Err(recovery) => Err(format!(
                        "Publication failed: {error}; recovery required on reopen: {recovery}"
                    )),
                };
            }
        }
        candidate.data_dir = self.path.clone();
        self.engine = candidate;
        Ok(())
    }
}

fn prepare(path: &Path, candidate: &mut KnotEngine) -> Result<()> {
    // Refuse overwriting any pending transaction. open() recovers it under lock.
    let journal = path.join(JOURNAL);
    std::fs::create_dir(&journal).map_err(err)?;
    private_dir(&journal)?;
    let result = (|| {
        let stage = journal.join("stage");
        let old = journal.join("old");
        std::fs::create_dir(&stage).map_err(err)?;
        std::fs::create_dir(&old).map_err(err)?;
        private_dir(&stage)?;
        private_dir(&old)?;
        let mut original = Vec::new();
        for name in ENTRIES {
            let source = path.join(name);
            if source.try_exists().map_err(err)? {
                original.push((*name).to_string());
                copy_private(&source, &stage.join(name))?;
            }
        }
        clear_record_materialization(&stage.join("repos"))?;
        candidate.data_dir = Some(stage.clone());
        candidate.persist().map_err(err)?;
        candidate.data_dir = None;
        verify_staged_snapshot(&stage, candidate)?;
        // fsync every staged file/directory before writing the rollback manifest.
        sync_tree(&stage)?;
        sync_dir(&old)?;
        let staged = ENTRIES
            .iter()
            .filter(|name| stage.join(name).exists())
            .map(|name| (*name).to_string())
            .collect();
        let manifest = Manifest { original, staged };
        let pending = journal.join("manifest.pending");
        let mut file = private_file(&pending, true)?;
        file.write_all(&serde_json::to_vec(&manifest).map_err(err)?)
            .map_err(err)?;
        file.sync_all().map_err(err)?;
        std::fs::rename(pending, journal.join("manifest.json")).map_err(err)?;
        sync_dir(&journal)?;
        sync_dir(path)
    })();
    if result.is_err() {
        candidate.data_dir = None;
        // No live files have moved before prepare returns successfully.
        let _ = std::fs::remove_dir_all(&journal);
    }
    result
}

// Validate the concrete on-disk snapshot before publishing it. In particular,
// core path escaping can alias distinct keys; refuse such unrepresentable data.
fn verify_staged_snapshot(stage: &Path, candidate: &KnotEngine) -> Result<()> {
    let did = candidate
        .describe
        .as_ref()
        .ok_or("Missing snapshot owner")?
        .did
        .as_str();
    let reopened = KnotEngine::open(stage, did).map_err(err)?;
    let records = |engine: &KnotEngine| -> Result<std::collections::BTreeMap<String, Value>> {
        engine
            .stored
            .iter()
            .map(|record| {
                Ok((
                    record.cid.clone(),
                    serde_json::to_value(record).map_err(err)?,
                ))
            })
            .collect()
    };
    if reopened.heads != candidate.heads
        || reopened.tips != candidate.tips
        || reopened.stored.len() != candidate.stored.len()
        || records(&reopened)? != records(candidate)?
        || reopened.blobs != candidate.blobs
        || reopened.blocks != candidate.blocks
        || reopened.revoked != candidate.revoked
    {
        return Err("Repository snapshot cannot faithfully represent candidate contents".into());
    }
    Ok(())
}

fn manifest(path: &Path) -> Result<Manifest> {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).map_err(err)?)
            .map_err(err)?;
    if manifest
        .original
        .iter()
        .chain(&manifest.staged)
        .any(|name| !ENTRIES.contains(&name.as_str()))
    {
        return Err("Invalid repository publication manifest".into());
    }
    Ok(manifest)
}

fn publish(path: &Path) -> Result<()> {
    let journal = path.join(JOURNAL);
    let manifest = manifest(&journal)?;
    let old = journal.join("old");
    let stage = journal.join("stage");
    for name in ENTRIES {
        if manifest.original.iter().any(|entry| entry == name) {
            std::fs::rename(path.join(name), old.join(name)).map_err(err)?;
            sync_dir(&old)?;
            sync_dir(path)?;
        }
        if manifest.staged.iter().any(|entry| entry == name) {
            std::fs::rename(stage.join(name), path.join(name)).map_err(err)?;
            sync_dir(&stage)?;
            sync_dir(path)?;
        }
    }
    // This marker is the commit point. If absent, recovery always restores old.
    let pending = journal.join("commit.pending");
    let file = private_file(&pending, true)?;
    file.sync_all().map_err(err)?;
    std::fs::rename(pending, journal.join("committed")).map_err(err)?;
    if let Err(error) = sync_dir(&journal) {
        // A failed commit-point flush is an error, so force rollback while old
        // files are still available instead of treating an unflushed marker as success.
        std::fs::remove_file(journal.join("committed")).map_err(err)?;
        sync_dir(&journal)?;
        return Err(error);
    }
    // A failed cleanup cannot invalidate an already committed snapshot. open()
    // retries cleanup before any subsequent repository operation.
    let _ = cleanup(path);
    Ok(())
}

pub(super) fn recover(path: &Path) -> Result<()> {
    let journal = path.join(JOURNAL);
    if !journal.try_exists().map_err(err)? {
        return Ok(());
    }
    if !journal.join("manifest.json").exists() {
        // Interrupted preparation has never moved live files.
        return cleanup(path);
    }
    if journal.join("committed").exists() {
        return cleanup(path);
    }
    let manifest = manifest(&journal)?;
    let old = journal.join("old");
    let stage = journal.join("stage");
    for name in ENTRIES {
        let live = path.join(name);
        let backup = old.join(name);
        if backup.try_exists().map_err(err)? {
            remove_if_exists(&live)?;
            std::fs::rename(&backup, &live).map_err(err)?;
            sync_dir(&old)?;
            sync_dir(path)?;
        } else if !manifest.original.iter().any(|entry| entry == name)
            && manifest.staged.iter().any(|entry| entry == name)
            && !stage.join(name).exists()
        {
            // Newly introduced entry was moved out of staging before interruption.
            remove_if_exists(&live)?;
            sync_dir(path)?;
        }
    }
    cleanup(path)
}

fn cleanup(path: &Path) -> Result<()> {
    // First remove the manifest only after rollback/commit is complete. A crash
    // during deletion then looks like interrupted preparation, which is safe.
    let journal = path.join(JOURNAL);
    if journal.join("manifest.json").exists() {
        std::fs::remove_file(journal.join("manifest.json")).map_err(err)?;
        sync_dir(&journal)?;
    }
    std::fs::remove_dir_all(journal).map_err(err)?;
    sync_dir(path)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path).map_err(err),
        Ok(_) => std::fs::remove_file(path).map_err(err),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(err(error)),
    }
}

// Core persist is additive. Remove its recognized record JSON/tip files in
// staging so absent authoritative keys cannot reappear when KnotEngine reopens.
// Keep directories (including obstructions that must fail persistence), opaque
// blocks, and unrelated files; signed historical records are rewritten below.
fn clear_record_materialization(repos: &Path) -> Result<()> {
    if !repos.exists() {
        return Ok(());
    }
    for repo in std::fs::read_dir(repos).map_err(err)? {
        let collections = repo.map_err(err)?.path().join("collections");
        if !collections.is_dir() {
            continue;
        }
        for collection in std::fs::read_dir(collections).map_err(err)? {
            let collection = collection.map_err(err)?;
            if !collection.file_type().map_err(err)?.is_dir() {
                continue;
            }
            for key in std::fs::read_dir(collection.path()).map_err(err)? {
                let key = key.map_err(err)?;
                if !key.file_type().map_err(err)?.is_dir() {
                    continue;
                }
                for file in std::fs::read_dir(key.path()).map_err(err)? {
                    let file = file.map_err(err)?;
                    let name = file.file_name();
                    let name = name.to_string_lossy();
                    if file.file_type().map_err(err)?.is_file()
                        && (name == "tip" || name.ends_with(".json"))
                    {
                        std::fs::remove_file(file.path()).map_err(err)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn copy_private(source: &Path, destination: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(source).map_err(err)?;
    if metadata.is_dir() {
        std::fs::create_dir(destination).map_err(err)?;
        private_dir(destination)?;
        for entry in std::fs::read_dir(source).map_err(err)? {
            let entry = entry.map_err(err)?;
            copy_private(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        std::fs::copy(source, destination).map_err(err)?;
        private_permissions(destination, 0o600)?;
    } else {
        return Err("Repository snapshot cannot stage symbolic links or special files".into());
    }
    Ok(())
}

fn sync_tree(path: &Path) -> Result<()> {
    for entry in std::fs::read_dir(path).map_err(err)? {
        let entry = entry.map_err(err)?;
        if entry.file_type().map_err(err)?.is_dir() {
            private_dir(&entry.path())?;
            sync_tree(&entry.path())?;
        } else {
            private_permissions(&entry.path(), 0o600)?;
            File::open(entry.path())
                .and_then(|file| file.sync_all())
                .map_err(err)?;
        }
    }
    sync_dir(path)
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Store, KnotEngine) {
        let dir = tempfile::tempdir().unwrap();
        let mut target = Store::open(dir.path()).unwrap();
        target
            .put("pub.tangled.note", "one", json!({"text":"old"}), "cap")
            .unwrap();
        let (ed, x) = target.identity.to_seeds();
        let mut source = Store {
            engine: target.engine.clone(),
            identity: Identity::from_seeds(ed, x),
            path: None,
            _lock: None,
        };
        source.engine.data_dir = None;
        source
            .put("pub.tangled.note", "one", json!({"text":"new"}), "cap")
            .unwrap();
        (dir, target, source.engine)
    }

    fn moved_entry(path: &Path, name: &str) {
        let journal = path.join(JOURNAL);
        if path.join(name).exists() {
            std::fs::rename(path.join(name), journal.join("old").join(name)).unwrap();
        }
        if journal.join("stage").join(name).exists() {
            std::fs::rename(journal.join("stage").join(name), path.join(name)).unwrap();
        }
    }

    #[test]
    fn reopening_recovers_each_interrupted_publication_prefix() {
        for prefix in 0..=ENTRIES.len() {
            let (dir, target, mut candidate) = fixture();
            let identity = std::fs::read(dir.path().join("identity.json")).unwrap();
            #[cfg(unix)]
            let inode = {
                use std::os::unix::fs::MetadataExt;
                std::fs::metadata(dir.path().join("personal.lock"))
                    .unwrap()
                    .ino()
            };
            prepare(dir.path(), &mut candidate).unwrap();
            for name in &ENTRIES[..prefix] {
                moved_entry(dir.path(), name);
            }
            drop(target);
            let mut reopened = Store::open(dir.path()).unwrap();
            assert_eq!(
                reopened.get("pub.tangled.note", "one").unwrap()["body"]["text"],
                "old",
                "prefix={prefix}"
            );
            assert_eq!(
                std::fs::read(dir.path().join("identity.json")).unwrap(),
                identity
            );
            assert!(!dir.path().join(JOURNAL).exists());
            assert!(Store::open(dir.path()).is_err());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(
                    std::fs::metadata(dir.path().join("personal.lock"))
                        .unwrap()
                        .ino(),
                    inode
                );
            }
        }
    }

    #[test]
    fn filesystem_error_mid_publish_rolls_back_and_reopens_with_old_contents() {
        let (dir, target, mut candidate) = fixture();
        let heads = std::fs::read(dir.path().join("heads.json")).unwrap();
        prepare(dir.path(), &mut candidate).unwrap();
        // A real rename error after metadata publication, before record publication.
        std::fs::remove_dir_all(dir.path().join(JOURNAL).join("stage/repos")).unwrap();
        assert!(publish(dir.path()).is_err());
        assert_ne!(std::fs::read(dir.path().join("heads.json")).unwrap(), heads);
        recover(dir.path()).unwrap();
        assert_eq!(std::fs::read(dir.path().join("heads.json")).unwrap(), heads);
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened.get("pub.tangled.note", "one").unwrap()["body"]["text"],
            "old"
        );
    }

    #[test]
    fn reopening_recovers_entry_moved_to_backup_before_replacement() {
        let (dir, target, mut candidate) = fixture();
        prepare(dir.path(), &mut candidate).unwrap();
        std::fs::rename(
            dir.path().join("heads.json"),
            dir.path().join(JOURNAL).join("old/heads.json"),
        )
        .unwrap();
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened.get("pub.tangled.note", "one").unwrap()["body"]["text"],
            "old"
        );
    }

    #[test]
    fn interrupted_first_import_removes_newly_introduced_snapshot_entries() {
        let dir = tempfile::tempdir().unwrap();
        let target = Store::open(dir.path()).unwrap();
        let (ed, x) = target.identity.to_seeds();
        let mut source = Store {
            engine: target.engine.clone(),
            identity: Identity::from_seeds(ed, x),
            path: None,
            _lock: None,
        };
        source.engine.data_dir = None;
        source
            .put("pub.tangled.note", "one", json!({"text":"new"}), "cap")
            .unwrap();
        prepare(dir.path(), &mut source.engine).unwrap();
        for name in ENTRIES {
            moved_entry(dir.path(), name);
        }
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert!(reopened.list(None).unwrap().is_empty());
        assert!(reopened.engine.heads.is_empty());
        assert!(!dir.path().join("repos").exists());
    }

    #[test]
    fn interrupted_preparation_leaves_prior_repository_usable() {
        let (dir, target, _) = fixture();
        let journal = dir.path().join(JOURNAL);
        std::fs::create_dir(&journal).unwrap();
        std::fs::create_dir(journal.join("stage")).unwrap();
        std::fs::write(journal.join("stage/heads.json"), b"partial JSON").unwrap();
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened.get("pub.tangled.note", "one").unwrap()["body"]["text"],
            "old"
        );
        assert!(!journal.exists());
    }

    #[test]
    fn committed_journal_reopens_new_contents_and_cleans_up() {
        let (dir, target, mut candidate) = fixture();
        prepare(dir.path(), &mut candidate).unwrap();
        for name in ENTRIES {
            moved_entry(dir.path(), name);
        }
        private_file(&dir.path().join(JOURNAL).join("committed"), true)
            .unwrap()
            .sync_all()
            .unwrap();
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened.get("pub.tangled.note", "one").unwrap()["body"]["text"],
            "new"
        );
        assert!(!dir.path().join(JOURNAL).exists());
    }
}
