//! Host-retained erasure. Fences deliberately survive deletion of the archive key.
use super::*;
use std::collections::HashSet;

#[derive(Debug, Default, Serialize)]
pub struct ErasureReceipt {
    pub records: usize,
    pub objects: usize,
    pub files: usize,
    pub batches: usize,
    pub chunks: usize,
    pub connections: usize,
    pub uploads: usize,
    pub generation: u64,
}

impl ArchiveStore {
    /// A durable owner-wide fence, including when no archive key remains.
    pub fn erasure_generation(&self, owner: &str) -> Result<u64, String> {
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS archive_erasure_generations(owner TEXT PRIMARY KEY,generation INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS archive_erased_records(owner TEXT NOT NULL,cid TEXT NOT NULL,PRIMARY KEY(owner,cid)); CREATE TABLE IF NOT EXISTS archive_retired_keys(owner TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(owner,digest));").map_err(err)?;
        Ok(self
            .db
            .query_row(
                "SELECT generation FROM archive_erasure_generations WHERE owner=?1",
                [owner],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?
            .unwrap_or(0))
    }

    pub fn check_erasure_generation(&self, owner: &str, generation: u64) -> Result<(), String> {
        if self.erasure_generation(owner)? != generation {
            return Err("Archive changed after erasure; this upload or backup is stale".into());
        }
        Ok(())
    }

    /// Permit portable post-erasure backups into a fresh host, while retaining local fences.
    /// The caller persists the returned generation in the same transaction as restored rows.
    pub fn check_restore_erasure(
        &self,
        owner: &str,
        backup: &ArchiveBackup,
    ) -> Result<u64, String> {
        let current = self.erasure_generation(owner)?;
        if backup.erasure_generation != current && (current != 0 || self.has_archive(owner)?) {
            return Err(
                "Archive backup predates or conflicts with this host's erasure fence".into(),
            );
        }
        if let Some(key) = backup.archive_key.as_deref() {
            let digest = opaque(&self.master, &decode_key(key)?);
            let retired:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM archive_retired_keys WHERE owner=?1 AND digest=?2)",params![owner,digest],|r|r.get(0)).map_err(err)?;
            if retired {
                return Err("This archive key was retired by whole-archive erasure".into());
            }
        }
        for record in backup
            .history
            .iter()
            .chain(backup.entries.iter().map(|(_, _, r)| r))
        {
            let cid = archive_record_cid(record)?;
            let erased: bool = self
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM archive_erased_records WHERE owner=?1 AND cid=?2)",
                    params![owner, cid],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if erased {
                return Err("Archive backup contains an erased private record".into());
            }
        }
        Ok(backup.erasure_generation)
    }

    pub fn erase_source(
        &mut self,
        owner: &str,
        id: &Identity,
        engine: &mut KnotEngine,
        source: &str,
        account: &str,
    ) -> Result<ErasureReceipt, String> {
        if !["telegram", "twitter", "whatsapp"].contains(&source) || account.trim().is_empty() {
            return Err("Choose a supported source and its stable account ID".into());
        }
        self.erase_scope(owner, id, engine, Some((source, account)))
    }

    pub fn erase_all(
        &mut self,
        owner: &str,
        id: &Identity,
        engine: &mut KnotEngine,
    ) -> Result<ErasureReceipt, String> {
        self.erase_scope(owner, id, engine, None)
    }

    fn erase_scope(
        &mut self,
        owner: &str,
        id: &Identity,
        engine: &mut KnotEngine,
        scope: Option<(&str, &str)>,
    ) -> Result<ErasureReceipt, String> {
        if id.did_string() != owner {
            return Err("Only the archive owner can erase private imports".into());
        }
        let generation = self
            .erasure_generation(owner)?
            .checked_add(1)
            .ok_or("Erasure generation overflow")?;
        let key = if self.has_archive(owner)? {
            Some(self.archive_key(owner, id)?)
        } else {
            None
        };
        // Gather and decrypt before destructive work. Corrupt records fail closed and remain retryable.
        let mut entries = self
            .db
            .prepare("SELECT kind,key,record FROM entries WHERE owner=?1")
            .map_err(err)?;
        let rows = entries
            .query_map([owner], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        drop(entries);
        let mut erase_entries = Vec::new();
        let mut erase_records = HashSet::new();
        let mut erase_chunks = HashSet::new();
        let mut keep_chunks = HashSet::new();
        let mut receipt = ErasureReceipt {
            generation,
            ..Default::default()
        };
        for (kind, logical, json) in rows {
            let rec: Value = serde_json::from_str(&json).map_err(err)?;
            if matches_scope(key.as_ref(), &rec, scope)? {
                match kind.as_str() {
                    OBJECT => receipt.objects += 1,
                    FILE => receipt.files += 1,
                    BATCH => receipt.batches += 1,
                    _ => {}
                }
                erase_records.insert(archive_record_cid(&rec)?);
                collect_chunks(&rec, &mut erase_chunks);
                erase_entries.push((kind, logical));
            } else {
                collect_chunks(&rec, &mut keep_chunks);
            }
        }
        let history = {
            let mut stmt = self
                .db
                .prepare("SELECT cid,record FROM history WHERE owner=?1")
                .map_err(err)?;
            let rows = stmt
                .query_map([owner], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            rows
        };
        for (cid, json) in history {
            let rec: Value = serde_json::from_str(&json).map_err(err)?;
            if matches_scope(key.as_ref(), &rec, scope)? {
                erase_records.insert(cid);
                collect_chunks(&rec, &mut erase_chunks);
            } else {
                collect_chunks(&rec, &mut keep_chunks);
            }
        }
        // Include accepted records from interrupted imports which never reached the DB index.
        for rec in engine.stored.iter().filter(|r| {
            r.did == owner && r.collection.starts_with("app.tangled.import.") && r.vis == "sealed"
        }) {
            if rec.collection == KEY {
                if scope.is_none() {
                    erase_records.insert(rec.cid.clone());
                }
            } else if matches_scope(key.as_ref(), &rec.record, scope)? {
                erase_records.insert(rec.cid.clone());
                collect_chunks(&rec.record, &mut erase_chunks);
            } else {
                collect_chunks(&rec.record, &mut keep_chunks);
            }
        }
        // Unreferenced staged archive chunks can result from interrupted imports. They
        // carry no source index, so collect owner-local orphans as well as scoped chunks.
        let mut stmt = self
            .db
            .prepare("SELECT cid FROM chunks WHERE owner=?1")
            .map_err(err)?;
        for row in stmt
            .query_map([owner], |r| r.get::<_, String>(0))
            .map_err(err)?
        {
            let cid = row.map_err(err)?;
            if !keep_chunks.contains(&cid) {
                erase_chunks.insert(cid);
            }
        }
        drop(stmt);
        // An engine write can precede the chunk index during a partial operation.
        if let Some(key) = key.as_ref() {
            for ((did, cid), bytes) in &engine.blobs {
                if did == owner && !keep_chunks.contains(cid) {
                    if let Ok(body) = serde_json::from_slice::<Value>(bytes) {
                        if open(key, b"tangled.import.blob.v1", &body).is_ok() {
                            erase_chunks.insert(cid.clone());
                        }
                    }
                }
            }
        }
        erase_chunks.retain(|cid| !keep_chunks.contains(cid));
        let connections = self
            .connections(owner)?
            .into_iter()
            .filter(|c| scope.is_none_or(|(s, a)| c.source == s && c.account == a))
            .collect::<Vec<_>>();
        // Commit the fence and revoke tokens first. A failed erasure cannot permit stale workers.
        let tx = self.db.transaction().map_err(err)?;
        tx.execute("INSERT INTO archive_erasure_generations VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET generation=excluded.generation",params![owner,generation]).map_err(err)?;
        for cid in &erase_records {
            tx.execute(
                "INSERT OR IGNORE INTO archive_erased_records VALUES(?1,?2)",
                params![owner, cid],
            )
            .map_err(err)?;
        }
        if scope.is_none() {
            if let Some(key) = key {
                tx.execute(
                    "INSERT OR IGNORE INTO archive_retired_keys VALUES(?1,?2)",
                    params![owner, opaque(&self.master, &key)],
                )
                .map_err(err)?;
            }
        }
        for c in &connections {
            tx.execute(
                "UPDATE connections SET active=0 WHERE id=?1 AND owner=?2",
                params![c.id, owner],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        let records = erase_records.iter().cloned().collect::<Vec<_>>();
        let chunks = erase_chunks.iter().cloned().collect::<Vec<_>>();
        engine.signing_keys.insert(owner.into(), id.to_seeds().0);
        engine
            .erase_private_archive(owner, &records, &chunks)
            .map_err(err)?;
        let upload_table:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='upload_sessions')",[],|r|r.get(0)).map_err(err)?;
        let tx = self.db.transaction().map_err(err)?;
        for (kind, logical) in erase_entries {
            if kind == OBJECT {
                tx.execute(
                    "INSERT OR IGNORE INTO deleted VALUES(?1,?2)",
                    params![owner, logical],
                )
                .map_err(err)?;
            }
            tx.execute(
                "DELETE FROM entries WHERE owner=?1 AND kind=?2 AND key=?3",
                params![owner, kind, logical],
            )
            .map_err(err)?;
        }
        for cid in &records {
            tx.execute(
                "DELETE FROM history WHERE owner=?1 AND cid=?2",
                params![owner, cid],
            )
            .map_err(err)?;
        }
        for cid in &chunks {
            receipt.chunks += tx
                .execute(
                    "DELETE FROM chunks WHERE owner=?1 AND cid=?2",
                    params![owner, cid],
                )
                .map_err(err)?;
        }
        for c in &connections {
            receipt.connections += tx
                .execute(
                    "DELETE FROM connections WHERE id=?1 AND owner=?2",
                    params![c.id, owner],
                )
                .map_err(err)?;
        }
        if upload_table {
            if let Some((s, _)) = scope {
                // Export account hints can differ from the authenticated source account.
                // Cancel pending uploads for this source; committed other accounts stay intact.
                tx.execute("DELETE FROM upload_chunks WHERE id IN(SELECT id FROM upload_sessions WHERE owner=?1 AND source=?2)",params![owner,s]).map_err(err)?;
                receipt.uploads = tx
                    .execute(
                        "DELETE FROM upload_sessions WHERE owner=?1 AND source=?2",
                        params![owner, s],
                    )
                    .map_err(err)?;
                // These sessions contain a different source and survive this erasure.
                // Rebase their fence atomically so they remain resumable and releasable.
                tx.execute(
                    "UPDATE upload_sessions SET generation=?1 WHERE owner=?2 AND source<>?3",
                    params![generation, owner, s],
                )
                .map_err(err)?;
            } else {
                tx.execute("DELETE FROM upload_chunks WHERE id IN(SELECT id FROM upload_sessions WHERE owner=?1)",[owner]).map_err(err)?;
                receipt.uploads = tx
                    .execute("DELETE FROM upload_sessions WHERE owner=?1", [owner])
                    .map_err(err)?;
            }
        }
        if scope.is_none() {
            for table in ["entries", "history", "chunks", "keyslots", "deleted"] {
                tx.execute(&format!("DELETE FROM {table} WHERE owner=?1"), [owner])
                    .map_err(err)?;
            }
        }
        let other_connections: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM connections WHERE owner=?1)",
                [owner],
                |r| r.get(0),
            )
            .map_err(err)?;
        if scope.is_none() || !other_connections {
            tx.execute("DELETE FROM signers WHERE owner=?1", [owner])
                .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        // secure_delete is enabled at open; VACUUM also removes retained free pages.
        self.db.execute_batch("VACUUM").map_err(err)?;
        receipt.records = records.len();
        Ok(receipt)
    }
}

fn matches_scope(
    key: Option<&[u8; 32]>,
    record: &Value,
    scope: Option<(&str, &str)>,
) -> Result<bool, String> {
    let Some((source, account)) = scope else {
        return Ok(true);
    };
    let value: Value = ArchiveStore::plaintext(
        key.ok_or("Missing archive key while erasing source")?,
        record,
    )?;
    let value = if record["collection"] == BATCH {
        &value["receipt"]
    } else {
        &value
    };
    Ok(value["source"].as_str() == Some(source) && value["account"].as_str() == Some(account))
}

fn collect_chunks(record: &Value, chunks: &mut HashSet<String>) {
    if let Some(cids) = record["body"]["blobs"].as_array() {
        chunks.extend(cids.iter().filter_map(Value::as_str).map(str::to_owned));
    }
}

fn archive_record_cid(record: &Value) -> Result<String, String> {
    let mut unsigned = record.clone();
    let sig = unsigned
        .as_object_mut()
        .and_then(|m| m.remove("sig"))
        .and_then(|s| s.as_str().map(str::to_owned))
        .ok_or("Missing private record signature")?;
    let sig: [u8; 64] = hex::decode(sig)
        .map_err(err)?
        .try_into()
        .map_err(|_| "Invalid private record signature")?;
    Ok(record::signed_record_cid(&unsigned, &sig)
        .map_err(err)?
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_erasure_preserves_chunks_referenced_by_other_account_history() {
        let dir = tempfile::tempdir().unwrap();
        let id = Identity::from_seeds([181; 32], [182; 32]);
        let owner = id.did_string();
        let mut store = ArchiveStore::open(dir.path()).unwrap();
        let mut engine = KnotEngine::hosting(&owner);
        engine.register_identity(&id);
        let key = store.archive_key(&owner, &id).unwrap();
        let manifest = store
            .store_file(
                &owner,
                &key,
                &mut engine,
                "shared",
                "application/octet-stream",
                b"shared attachment",
            )
            .unwrap();
        for account in ["a", "b"] {
            store
                .write(
                    &owner,
                    &id,
                    &key,
                    &mut engine,
                    FILE,
                    account,
                    &json!({"source":"telegram","account":account,"file":manifest}),
                    &manifest.chunks,
                )
                .unwrap();
        }
        let erased = store
            .erase_source(&owner, &id, &mut engine, "telegram", "a")
            .unwrap();
        assert_eq!(erased.files, 1);
        let rec = store.entry(&owner, FILE, "b").unwrap().unwrap();
        let value: Value = ArchiveStore::plaintext(&key, &rec).unwrap();
        let file: FileManifest = serde_json::from_value(value["file"].clone()).unwrap();
        assert_eq!(
            store.read_file(&owner, &key, &file).unwrap(),
            b"shared attachment"
        );
        for cid in manifest.chunks {
            assert!(engine.blobs.contains_key(&(owner.clone(), cid.clone())));
            assert!(engine.blocks.contains_key(&cid));
        }
    }
}
