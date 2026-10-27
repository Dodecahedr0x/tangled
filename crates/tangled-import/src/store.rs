//! Encrypted archive storage and sealed repository projection.
pub mod erasure;
pub mod uploads;
use crate::{ImportBatch, ImportedObject};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tangled::{cid::Cid, hpke, identity::Identity, knot::KnotEngine, record};

const OBJECT: &str = "app.tangled.import.object";
const FILE: &str = "app.tangled.import.file";
const BATCH: &str = "app.tangled.import.batch";
const KEY: &str = "app.tangled.import.key";
const CHUNK: usize = 512 * 1024;
pub const MAX_UPLOAD: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub source: String,
    pub account: String,
    pub filename: String,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub files: usize,
    pub warnings: Vec<String>,
    pub imported_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveObject {
    pub key: String,
    pub source: String,
    pub account: String,
    pub object: ImportedObject,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveFileMetadata {
    pub key: String,
    pub source: String,
    pub account: String,
    pub name: String,
    pub media_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceConnection {
    pub id: String,
    pub owner: String,
    pub source: String,
    pub account: String,
    pub status: String,
    pub created_at: u64,
    pub last_ingest: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Connected {
    pub connection: SourceConnection,
    pub token: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveBackup {
    pub version: u32,
    pub owner: String,
    pub archive_key: Option<String>,
    pub entries: Vec<(String, String, Value)>,
    pub chunks: Vec<(String, String)>,
    pub deleted: Vec<String>,
    #[serde(default)]
    pub history: Vec<Value>,
    #[serde(default)]
    pub erasure_generation: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct FileManifest {
    name: String,
    media_type: String,
    chunks: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedBatch {
    receipt: Receipt,
    original: FileManifest,
}
pub struct ArchiveStore {
    db: Connection,
    master: [u8; 32],
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn random_key() -> [u8; 32] {
    let mut key = [0; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}
fn recipient(key: &[u8; 32]) -> Identity {
    Identity::from_seeds([0; 32], *key)
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(err)
}
fn seal(pk: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Value, String> {
    let (enc, ct) = hpke::seal_record(pk, aad, plaintext).map_err(err)?;
    Ok(json!({"alg":"hpke-record","enc":hex::encode(enc),"ct":hex::encode(ct)}))
}
fn open(sk: &[u8; 32], aad: &[u8], body: &Value) -> Result<Vec<u8>, String> {
    let enc = hex::decode(body["enc"].as_str().ok_or("Missing encapsulation")?).map_err(err)?;
    let ct = hex::decode(body["ct"].as_str().ok_or("Missing ciphertext")?).map_err(err)?;
    hpke::open_record(sk, &enc, aad, &ct).map_err(err)
}
fn decode_key(value: &str) -> Result<[u8; 32], String> {
    hex::decode(value)
        .map_err(err)?
        .try_into()
        .map_err(|_| "Invalid archive key".into())
}
fn opaque(key: &[u8; 32], data: &[u8]) -> String {
    blake3::keyed_hash(key, data).to_hex().to_string()
}

fn source_instant(object: &ImportedObject) -> Option<i128> {
    if let Some(value) = object.data.get("date_unixtime") {
        let seconds = value
            .as_i64()
            .or_else(|| value.as_str()?.parse::<i64>().ok());
        if let Some(seconds) = seconds {
            return Some(i128::from(seconds) * 1_000_000_000);
        }
    }
    let literal = object.timestamp.as_deref()?;
    if let Ok(time) =
        time::OffsetDateTime::parse(literal, &time::format_description::well_known::Rfc3339)
    {
        return Some(time.unix_timestamp_nanos());
    }
    if let Ok(time) =
        time::OffsetDateTime::parse(literal, &time::format_description::well_known::Rfc2822)
    {
        return Some(time.unix_timestamp_nanos());
    }
    let format=time::format_description::parse_borrowed::<2>("[weekday repr:short] [month repr:short] [day] [hour]:[minute]:[second] [offset_hour sign:mandatory][offset_minute] [year]").ok()?;
    time::OffsetDateTime::parse(literal, &format)
        .ok()
        .map(|t| t.unix_timestamp_nanos())
}

impl ArchiveStore {
    pub fn open(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(err)?;
        let key_path = dir.join("archive-host.key");
        let master = match std::fs::read(&key_path) {
            Ok(bytes) => bytes.try_into().map_err(|_| "Invalid archive host key")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key = random_key();
                use std::io::Write;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                options
                    .open(&key_path)
                    .map_err(err)?
                    .write_all(&key)
                    .map_err(err)?;
                key
            }
            Err(e) => return Err(err(e)),
        };
        let db_path = dir.join("imports.db");
        let db = Connection::open(&db_path).map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o600))
                .map_err(err)?;
        }
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA secure_delete=ON;
          CREATE TABLE IF NOT EXISTS signers(owner TEXT PRIMARY KEY, capsule TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS keyslots(owner TEXT PRIMARY KEY, capsule TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS history(owner TEXT NOT NULL, cid TEXT NOT NULL, record TEXT NOT NULL, PRIMARY KEY(owner,cid));
          CREATE TABLE IF NOT EXISTS entries(owner TEXT NOT NULL, kind TEXT NOT NULL, key TEXT NOT NULL, record TEXT NOT NULL, PRIMARY KEY(owner,kind,key));
          CREATE TABLE IF NOT EXISTS chunks(owner TEXT NOT NULL, cid TEXT NOT NULL, bytes BLOB NOT NULL, PRIMARY KEY(owner,cid));
          CREATE TABLE IF NOT EXISTS connections(id TEXT PRIMARY KEY, owner TEXT NOT NULL, token_hash TEXT NOT NULL, active INTEGER NOT NULL, capsule TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS deleted(owner TEXT NOT NULL, key TEXT NOT NULL, PRIMARY KEY(owner,key));").map_err(err)?;
        let mut store = Self { db, master };
        store.expire_uploads()?;
        Ok(store)
    }
    fn archive_key(&self, owner: &str, id: &Identity) -> Result<[u8; 32], String> {
        let stored: Option<String> = self
            .db
            .query_row(
                "SELECT capsule FROM keyslots WHERE owner=?1",
                [owner],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if let Some(capsule) = stored {
            return open(
                &id.to_seeds().1,
                owner.as_bytes(),
                &serde_json::from_str::<Value>(&capsule).map_err(err)?,
            )?
            .try_into()
            .map_err(|_| "Invalid decrypted archive key".into());
        }
        let key = random_key();
        self.set_key(owner, id, &key)?;
        Ok(key)
    }
    fn set_key(&self, owner: &str, id: &Identity, key: &[u8; 32]) -> Result<(), String> {
        let capsule = seal(&id.agreement_public_bytes(), owner.as_bytes(), key)?;
        self.db
            .execute(
                "INSERT OR REPLACE INTO keyslots VALUES(?1,?2)",
                params![owner, capsule.to_string()],
            )
            .map_err(err)?;
        Ok(())
    }
    fn has_archive(&self, owner: &str) -> Result<bool, String> {
        self.db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM keyslots WHERE owner=?1)",
                [owner],
                |r| r.get(0),
            )
            .map_err(err)
    }
    fn entry(&self, owner: &str, kind: &str, key: &str) -> Result<Option<Value>, String> {
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT record FROM entries WHERE owner=?1 AND kind=?2 AND key=?3",
                params![owner, kind, key],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        value
            .map(|s| serde_json::from_str(&s).map_err(err))
            .transpose()
    }
    fn values(&self, owner: &str, kind: &str) -> Result<Vec<Value>, String> {
        let mut stmt = self
            .db
            .prepare("SELECT record FROM entries WHERE owner=?1 AND kind=?2 ORDER BY key")
            .map_err(err)?;
        let rows = stmt
            .query_map(params![owner, kind], |r| r.get::<_, String>(0))
            .map_err(err)?;
        rows.map(|r| serde_json::from_str(&r.map_err(err)?).map_err(err))
            .collect()
    }
    fn plaintext<T: for<'de> Deserialize<'de>>(key: &[u8; 32], rec: &Value) -> Result<T, String> {
        let aad = record::unsigned_without_body_and_sig(rec).map_err(err)?;
        serde_json::from_slice(&open(key, &aad, &rec["body"])?).map_err(err)
    }
    // This helper accepts the record's owner, signing/encryption context, and payload.
    #[allow(clippy::too_many_arguments)]
    fn write<T: Serialize>(
        &self,
        owner: &str,
        signer: &Identity,
        key: &[u8; 32],
        engine: &mut KnotEngine,
        collection: &str,
        logical: &str,
        data: &T,
        blobs: &[String],
    ) -> Result<(), String> {
        let rkey = opaque(key, logical.as_bytes());
        let previous = engine
            .tips
            .get(&(owner.into(), collection.into(), rkey.clone()))
            .cloned();
        let mut unsigned = json!({"v":1,"type":Cid::dag_cbor(collection.as_bytes()).to_string(),"collection":collection,
            "rkey":rkey,"author":owner,"ts":now(),"prev":previous.into_iter().collect::<Vec<_>>(),"vis":"sealed","body":{}});
        let aad = record::unsigned_without_body_and_sig(&unsigned).map_err(err)?;
        unsigned["body"] = seal(
            &recipient(key).agreement_public_bytes(),
            &aad,
            &encode(data)?,
        )?;
        unsigned["body"]["blobs"] = json!(blobs);
        let signed = record::sign_json_record(signer.signing_key(), &unsigned).map_err(err)?;
        engine
            .signing_keys
            .insert(owner.into(), signer.to_seeds().0);
        let accepted = engine.ingest_record(signed.json.clone()).map_err(err)?;
        self.db
            .execute(
                "INSERT OR IGNORE INTO history VALUES(?1,?2,?3)",
                params![owner, accepted.cid, signed.json.to_string()],
            )
            .map_err(err)?;
        self.db
            .execute(
                "INSERT OR REPLACE INTO entries VALUES(?1,?2,?3,?4)",
                params![owner, collection, logical, signed.json.to_string()],
            )
            .map_err(err)?;
        Ok(())
    }
    fn store_file(
        &self,
        owner: &str,
        key: &[u8; 32],
        engine: &mut KnotEngine,
        name: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<FileManifest, String> {
        let mut chunks = Vec::new();
        for chunk in bytes.chunks(CHUNK) {
            let encrypted = encode(&seal(
                &recipient(key).agreement_public_bytes(),
                b"tangled.import.blob.v1",
                chunk,
            )?)?;
            let cid = Cid::raw(&encrypted).to_string();
            self.db
                .execute(
                    "INSERT OR IGNORE INTO chunks VALUES(?1,?2,?3)",
                    params![owner, cid, encrypted],
                )
                .map_err(err)?;
            engine
                .blobs
                .insert((owner.into(), cid.clone()), encrypted.clone());
            engine.blocks.insert(cid.clone(), encrypted);
            engine.blob_types.insert(
                (owner.into(), cid.clone()),
                "application/octet-stream".into(),
            );
            chunks.push(cid);
        }
        Ok(FileManifest {
            name: name.into(),
            media_type: media_type.into(),
            chunks,
        })
    }
    fn read_file(
        &self,
        owner: &str,
        key: &[u8; 32],
        file: &FileManifest,
    ) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        for cid in &file.chunks {
            let bytes: Vec<u8> = self
                .db
                .query_row(
                    "SELECT bytes FROM chunks WHERE owner=?1 AND cid=?2",
                    params![owner, cid],
                    |r| r.get(0),
                )
                .map_err(err)?;
            let sealed: Value = serde_json::from_slice(&bytes).map_err(err)?;
            out.extend(open(key, b"tangled.import.blob.v1", &sealed)?);
        }
        Ok(out)
    }
    pub fn ingest(
        &mut self,
        owner: &str,
        signer: &Identity,
        engine: &mut KnotEngine,
        filename: &str,
        raw: &[u8],
        batch: &ImportBatch,
    ) -> Result<Receipt, String> {
        validate_batch(batch)?;
        // Export filenames are not globally unique (especially WhatsApp media).
        // Bind each normalized attachment to immutable content, retaining raw source names in the upload.
        let mut normalized = batch.clone();
        let mut aliases = std::collections::HashMap::new();
        let mut expanded = 0usize;
        for file in &mut normalized.files {
            let bytes = STANDARD.decode(&file.data_base64).map_err(err)?;
            expanded = expanded
                .checked_add(bytes.len())
                .ok_or("Expanded media size overflow")?;
            if expanded > MAX_UPLOAD {
                return Err("Split this import into smaller media batches".into());
            }
            let alias = format!("{}/{}", blake3::hash(&bytes).to_hex(), file.name);
            aliases.insert(file.name.clone(), alias.clone());
            file.name = alias;
        }
        for object in &mut normalized.objects {
            for attachment in &mut object.attachments {
                if let Some(alias) = aliases.get(attachment) {
                    *attachment = alias.clone();
                }
            }
        }
        let batch = &normalized;
        if raw.len() > MAX_UPLOAD {
            return Err("Upload exceeds 256 MiB; split the archive into smaller exports".into());
        }
        let archive_key = self.archive_key(owner, signer)?;
        let batch_id = opaque(
            &archive_key,
            &encode(&(
                batch.source.as_str(),
                batch.account.as_str(),
                filename,
                blake3::hash(raw).to_hex().to_string(),
            ))?,
        );
        if let Some(rec) = self.entry(owner, BATCH, &batch_id)? {
            let mut prior: SavedBatch = Self::plaintext(&archive_key, &rec)?;
            prior.receipt.unchanged += prior.receipt.inserted + prior.receipt.updated;
            prior.receipt.inserted = 0;
            prior.receipt.updated = 0;
            return Ok(prior.receipt);
        }
        let mut receipt = Receipt {
            id: batch_id.clone(),
            source: batch.source.clone(),
            account: batch.account.clone(),
            filename: filename.into(),
            inserted: 0,
            updated: 0,
            unchanged: 0,
            files: batch.files.len(),
            warnings: batch.warnings.clone(),
            imported_at: now(),
        };
        // The archive key capsule is portable alongside the encrypted objects.
        let capsule: String = self
            .db
            .query_row(
                "SELECT capsule FROM keyslots WHERE owner=?1",
                [owner],
                |r| r.get(0),
            )
            .map_err(err)?;
        let key_body: Value = serde_json::from_str(&capsule).map_err(err)?;
        let key_logical = "archive-key";
        let mut unsigned = json!({"v":1,"type":Cid::dag_cbor(KEY.as_bytes()).to_string(),"collection":KEY,"rkey":opaque(&archive_key,key_logical.as_bytes()),"author":owner,"ts":now(),"prev":[],"vis":"sealed","body":key_body});
        if let Some(prev) = engine.tips.get(&(
            owner.into(),
            KEY.into(),
            unsigned["rkey"].as_str().unwrap_or_default().into(),
        )) {
            unsigned["prev"] = json!([prev]);
        }
        let signed = record::sign_json_record(signer.signing_key(), &unsigned).map_err(err)?;
        engine
            .signing_keys
            .insert(owner.into(), signer.to_seeds().0);
        engine.ingest_record(signed.json).map_err(err)?;
        for object in &batch.objects {
            let logical = opaque(
                &archive_key,
                &encode(&(
                    batch.source.as_str(),
                    batch.account.as_str(),
                    object.kind.as_str(),
                    identity_conversation(&batch.source, object),
                    object.id.as_str(),
                ))?,
            );
            let deleted: bool = self
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM deleted WHERE owner=?1 AND key=?2)",
                    params![owner, logical],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if deleted {
                receipt.unchanged += 1;
                continue;
            }
            let value = ArchiveObject {
                key: logical.clone(),
                source: batch.source.clone(),
                account: batch.account.clone(),
                object: object.clone(),
            };
            if let Some(prior) = self.entry(owner, OBJECT, &logical)? {
                let prior: ArchiveObject = Self::plaintext(&archive_key, &prior)?;
                if encode(&prior.object)? == encode(object)? {
                    receipt.unchanged += 1;
                    continue;
                }
                let prior_edit = source_revision(&batch.source, &prior.object);
                let next_edit = source_revision(&batch.source, object);
                let conflict = if missing_media_enrichment(&prior.object, object) {
                    None
                } else {
                    match (prior_edit, next_edit) {
                        (Some(prior), Some(next)) if next > prior => None,
                        (Some(prior), Some(next)) if next < prior => {
                            Some("Kept newer source revision")
                        }
                        _ => {
                            Some("Kept existing object because source revision order is uncertain")
                        }
                    }
                };
                if let Some(reason) = conflict {
                    receipt.unchanged += 1;
                    receipt.warnings.push(format!(
                        "{reason} for {}; incoming source bytes are retained in this import",
                        object.id
                    ));
                    continue;
                }
                receipt.updated += 1;
            } else {
                receipt.inserted += 1;
            }
            self.write(
                owner,
                signer,
                &archive_key,
                engine,
                OBJECT,
                &logical,
                &value,
                &[],
            )?;
        }
        for file in &batch.files {
            let bytes = STANDARD.decode(&file.data_base64).map_err(err)?;
            if bytes.len() > MAX_UPLOAD {
                return Err("Attachment exceeds archive limit".into());
            }
            let file_id = opaque(
                &archive_key,
                &encode(&(
                    batch.source.as_str(),
                    batch.account.as_str(),
                    file.name.as_str(),
                ))?,
            );
            let manifest = self.store_file(
                owner,
                &archive_key,
                engine,
                &file.name,
                &file.media_type,
                &bytes,
            )?;
            self.write(
                owner,
                signer,
                &archive_key,
                engine,
                FILE,
                &file_id,
                &json!({"source":batch.source,"account":batch.account,"file":manifest}),
                &manifest.chunks,
            )?;
        }
        let original = self.store_file(
            owner,
            &archive_key,
            engine,
            filename,
            "application/octet-stream",
            raw,
        )?;
        let saved = SavedBatch {
            receipt: receipt.clone(),
            original,
        };
        self.write(
            owner,
            signer,
            &archive_key,
            engine,
            BATCH,
            &batch_id,
            &saved,
            &saved.original.chunks,
        )?;
        engine.persist().map_err(err)?;
        Ok(receipt)
    }
    pub fn objects(&self, owner: &str, id: &Identity) -> Result<Vec<ArchiveObject>, String> {
        if !self.has_archive(owner)? {
            return Ok(vec![]);
        }
        let key = self.archive_key(owner, id)?;
        let mut objects: Vec<ArchiveObject> = self
            .values(owner, OBJECT)?
            .iter()
            .map(|r| Self::plaintext(&key, r))
            .collect::<Result<_, _>>()?;
        objects.sort_by(|a, b| {
            source_instant(&b.object)
                .cmp(&source_instant(&a.object))
                .then(a.key.cmp(&b.key))
        });
        Ok(objects)
    }
    pub fn receipts(&self, owner: &str, id: &Identity) -> Result<Vec<Receipt>, String> {
        if !self.has_archive(owner)? {
            return Ok(vec![]);
        }
        let key = self.archive_key(owner, id)?;
        let mut out: Vec<Receipt> = self
            .values(owner, BATCH)?
            .iter()
            .map(|r| Self::plaintext::<SavedBatch>(&key, r).map(|b| b.receipt))
            .collect::<Result<_, _>>()?;
        out.sort_by_key(|r| std::cmp::Reverse(r.imported_at));
        Ok(out)
    }
    pub fn original(
        &self,
        owner: &str,
        id: &Identity,
        batch_id: &str,
    ) -> Result<(String, Vec<u8>), String> {
        if !self.has_archive(owner)? {
            return Err("Archive not found".into());
        }
        let key = self.archive_key(owner, id)?;
        let rec = self
            .entry(owner, BATCH, batch_id)?
            .ok_or("Import not found")?;
        let batch: SavedBatch = Self::plaintext(&key, &rec)?;
        Ok((
            batch.original.name.clone(),
            self.read_file(owner, &key, &batch.original)?,
        ))
    }
    pub fn attachment(
        &self,
        owner: &str,
        id: &Identity,
        source: &str,
        account: &str,
        name: &str,
    ) -> Result<(String, Vec<u8>), String> {
        let key = self.archive_key(owner, id)?;
        let file_id = opaque(&key, &encode(&(source, account, name))?);
        let rec = self
            .entry(owner, FILE, &file_id)?
            .ok_or("Attachment not found")?;
        let value: Value = Self::plaintext(&key, &rec)?;
        let manifest: FileManifest = serde_json::from_value(value["file"].clone()).map_err(err)?;
        Ok((
            manifest.media_type.clone(),
            self.read_file(owner, &key, &manifest)?,
        ))
    }
    pub fn delete_object(&self, owner: &str, object: &str) -> Result<(), String> {
        self.db
            .execute(
                "INSERT OR IGNORE INTO deleted VALUES(?1,?2)",
                params![owner, object],
            )
            .map_err(err)?;
        self.db
            .execute(
                "DELETE FROM entries WHERE owner=?1 AND kind=?2 AND key=?3",
                params![owner, OBJECT, object],
            )
            .map_err(err)?;
        Ok(())
    }
    pub fn connect(
        &mut self,
        owner: &str,
        source: &str,
        account: &str,
    ) -> Result<Connected, String> {
        if !["telegram", "twitter", "whatsapp"].contains(&source)
            || account.trim().is_empty()
            || account.len() > 256
        {
            return Err("Choose a supported source and its stable account ID".into());
        }
        let connection = SourceConnection {
            id: hex::encode(random_key()),
            owner: owner.into(),
            source: source.into(),
            account: account.trim().into(),
            status: "awaiting_connector".into(),
            created_at: now(),
            last_ingest: None,
        };
        let token = hex::encode(random_key());
        let capsule = seal(
            &recipient(&self.master).agreement_public_bytes(),
            connection.id.as_bytes(),
            &encode(&connection)?,
        )?;
        self.db
            .execute(
                "INSERT INTO connections VALUES(?1,?2,?3,1,?4)",
                params![
                    connection.id,
                    owner,
                    opaque(&self.master, token.as_bytes()),
                    capsule.to_string()
                ],
            )
            .map_err(err)?;
        Ok(Connected { connection, token })
    }
    fn connection(&self, id: &str) -> Result<SourceConnection, String> {
        let (capsule, active): (String, bool) = self
            .db
            .query_row(
                "SELECT capsule,active FROM connections WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)?;
        let mut c: SourceConnection = serde_json::from_slice(&open(
            &self.master,
            id.as_bytes(),
            &serde_json::from_str::<Value>(&capsule).map_err(err)?,
        )?)
        .map_err(err)?;
        if !active {
            c.status = "disconnected".into();
        }
        Ok(c)
    }
    pub fn connections(&self, owner: &str) -> Result<Vec<SourceConnection>, String> {
        let mut stmt = self
            .db
            .prepare("SELECT id FROM connections WHERE owner=?1")
            .map_err(err)?;
        let ids = stmt
            .query_map([owner], |r| r.get::<_, String>(0))
            .map_err(err)?;
        ids.map(|id| self.connection(&id.map_err(err)?)).collect()
    }
    pub fn authorize_ingest(
        &self,
        connection: &str,
        token: &str,
    ) -> Result<SourceConnection, String> {
        let valid: bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM connections WHERE id=?1 AND token_hash=?2 AND active=1)",params![connection,opaque(&self.master,token.as_bytes())],|r|r.get(0)).map_err(err)?;
        if !valid {
            return Err("Connection token is invalid or disconnected".into());
        }
        self.connection(connection)
    }
    pub fn mark_ingested(&self, id: &str) -> Result<(), String> {
        let mut connection = self.connection(id)?;
        connection.last_ingest = Some(now());
        connection.status = "connected".into();
        let capsule = seal(
            &recipient(&self.master).agreement_public_bytes(),
            id.as_bytes(),
            &encode(&connection)?,
        )?;
        self.db
            .execute(
                "UPDATE connections SET capsule=?1 WHERE id=?2 AND active=1",
                params![capsule.to_string(), id],
            )
            .map_err(err)?;
        Ok(())
    }
    pub fn disconnect(&mut self, owner: &str, id: &str) -> Result<(), String> {
        let count = self
            .db
            .execute(
                "UPDATE connections SET active=0,token_hash='' WHERE id=?1 AND owner=?2",
                params![id, owner],
            )
            .map_err(err)?;
        if count == 0 {
            return Err("Connection not found".into());
        }
        Ok(())
    }
    pub fn disconnect_all(&mut self, owner: &str) -> Result<(), String> {
        self.db
            .execute(
                "UPDATE connections SET active=0,token_hash='' WHERE owner=?1",
                [owner],
            )
            .map_err(err)?;
        self.db
            .execute("DELETE FROM signers WHERE owner=?1", [owner])
            .map_err(err)?;
        Ok(())
    }
    pub fn backup(&self, owner: &str, id: &Identity) -> Result<ArchiveBackup, String> {
        let archive_key = if self.has_archive(owner)? {
            Some(hex::encode(self.archive_key(owner, id)?))
        } else {
            None
        };
        let mut stmt = self
            .db
            .prepare("SELECT kind,key,record FROM entries WHERE owner=?1")
            .map_err(err)?;
        let entries = stmt
            .query_map([owner], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?
            .map(|r| {
                let (kind, key, value) = r.map_err(err)?;
                Ok((kind, key, serde_json::from_str(&value).map_err(err)?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut stmt = self
            .db
            .prepare("SELECT cid,bytes FROM chunks WHERE owner=?1")
            .map_err(err)?;
        let chunks = stmt
            .query_map([owner], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(err)?
            .map(|r| {
                r.map(|(cid, bytes)| (cid, STANDARD.encode(bytes)))
                    .map_err(err)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = self
            .db
            .prepare("SELECT key FROM deleted WHERE owner=?1")
            .map_err(err)?;
        let deleted = stmt
            .query_map([owner], |r| r.get(0))
            .map_err(err)?
            .collect::<Result<Vec<String>, _>>()
            .map_err(err)?;
        let mut stmt = self
            .db
            .prepare("SELECT record FROM history WHERE owner=?1 ORDER BY rowid")
            .map_err(err)?;
        let history = stmt
            .query_map([owner], |r| r.get::<_, String>(0))
            .map_err(err)?
            .map(|r| serde_json::from_str(&r.map_err(err)?).map_err(err))
            .collect::<Result<Vec<Value>, String>>()?;
        Ok(ArchiveBackup {
            version: 1,
            owner: owner.into(),
            archive_key,
            entries,
            chunks,
            deleted,
            history,
            erasure_generation: self.erasure_generation(owner)?,
        })
    }
    pub fn restore(
        &mut self,
        owner: &str,
        id: &Identity,
        backup: &ArchiveBackup,
    ) -> Result<(), String> {
        let restore_generation = self.check_restore_erasure(owner, backup)?;
        if backup.version != 1 || backup.owner != owner {
            return Err("Archive backup owner/version mismatch".into());
        }
        let key = backup.archive_key.as_deref().map(decode_key).transpose()?;
        if self.has_archive(owner)? && key.as_ref() != Some(&self.archive_key(owner, id)?) {
            return Err("This account already has a different private archive. Restore into an empty account archive instead of replacing its key.".into());
        }
        for (kind, logical, record) in &backup.entries {
            if record["author"].as_str() != Some(owner)
                || record["vis"] != "sealed"
                || record["collection"].as_str() != Some(kind.as_str())
                || ![OBJECT, FILE, BATCH].contains(&kind.as_str())
            {
                return Err("Invalid private archive record".into());
            }
            let archive_key = key.as_ref().ok_or("Missing archive key")?;
            if record["rkey"].as_str() != Some(opaque(archive_key, logical.as_bytes()).as_str()) {
                return Err("Archive index mismatch".into());
            }
            let _: Value = Self::plaintext(archive_key, record)?;
        }
        for record in &backup.history {
            if record["author"].as_str() != Some(owner) || record["vis"] != "sealed" {
                return Err("Invalid archive history".into());
            }
            let _: Value = Self::plaintext(key.as_ref().ok_or("Missing archive key")?, record)?;
        }
        let chunks = backup
            .chunks
            .iter()
            .map(|(cid, bytes)| {
                let bytes = STANDARD.decode(bytes).map_err(err)?;
                if Cid::raw(&bytes).to_string() != *cid {
                    return Err("Archive blob checksum mismatch".into());
                }
                Ok((cid.clone(), bytes))
            })
            .collect::<Result<Vec<_>, String>>()?;
        // Validate all retained manifests before changing an existing destination.
        let chunk_map = chunks
            .iter()
            .map(|(c, b)| (c.as_str(), b))
            .collect::<std::collections::HashMap<_, _>>();
        for (kind, _, record) in &backup.entries {
            let value: Value = Self::plaintext(key.as_ref().ok_or("Missing archive key")?, record)?;
            let manifest = if kind == FILE {
                Some(value["file"].clone())
            } else if kind == BATCH {
                Some(value["original"].clone())
            } else {
                None
            };
            if let Some(manifest) = manifest {
                let manifest: FileManifest = serde_json::from_value(manifest).map_err(err)?;
                for cid in manifest.chunks {
                    let bytes = chunk_map
                        .get(cid.as_str())
                        .ok_or("Archive manifest references a missing stored chunk")?;
                    let body: Value = serde_json::from_slice(bytes).map_err(err)?;
                    open(
                        key.as_ref().ok_or("Missing archive key")?,
                        b"tangled.import.blob.v1",
                        &body,
                    )?;
                }
            }
        }
        let tx = self.db.transaction().map_err(err)?;
        tx.execute("INSERT INTO archive_erasure_generations VALUES(?1,?2) ON CONFLICT(owner) DO UPDATE SET generation=excluded.generation",params![owner,restore_generation]).map_err(err)?;
        if let Some(key) = key {
            let capsule = seal(&id.agreement_public_bytes(), owner.as_bytes(), &key)?;
            tx.execute(
                "INSERT OR REPLACE INTO keyslots VALUES(?1,?2)",
                params![owner, capsule.to_string()],
            )
            .map_err(err)?;
        }
        for (kind, logical, record) in &backup.entries {
            tx.execute(
                "INSERT OR REPLACE INTO entries VALUES(?1,?2,?3,?4)",
                params![owner, kind, logical, record.to_string()],
            )
            .map_err(err)?;
        }
        for record in &backup.history {
            let mut unsigned = record.clone();
            let sig = unsigned
                .as_object_mut()
                .and_then(|v| v.remove("sig"))
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or("Missing record signature")?;
            let sig: [u8; 64] = hex::decode(sig)
                .map_err(err)?
                .try_into()
                .map_err(|_| "Invalid record signature")?;
            let cid = record::signed_record_cid(&unsigned, &sig)
                .map_err(err)?
                .to_string();
            tx.execute(
                "INSERT OR IGNORE INTO history VALUES(?1,?2,?3)",
                params![owner, cid, record.to_string()],
            )
            .map_err(err)?;
        }
        for (cid, bytes) in chunks {
            tx.execute(
                "INSERT OR IGNORE INTO chunks VALUES(?1,?2,?3)",
                params![owner, cid, bytes],
            )
            .map_err(err)?;
        }
        for logical in &backup.deleted {
            tx.execute(
                "INSERT OR IGNORE INTO deleted VALUES(?1,?2)",
                params![owner, logical],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(())
    }
    pub fn project_backup(
        &self,
        owner: &str,
        backup: &ArchiveBackup,
        engine: &mut KnotEngine,
    ) -> Result<(), String> {
        for (cid, bytes) in &backup.chunks {
            let bytes = STANDARD.decode(bytes).map_err(err)?;
            engine
                .blobs
                .insert((owner.into(), cid.clone()), bytes.clone());
            engine.blocks.insert(cid.clone(), bytes);
        }
        let records = if backup.history.is_empty() {
            backup
                .entries
                .iter()
                .map(|(_, _, r)| r.clone())
                .collect::<Vec<_>>()
        } else {
            backup.history.clone()
        };
        for record in records {
            let key = (
                owner.to_string(),
                record["collection"]
                    .as_str()
                    .ok_or("Missing collection")?
                    .to_string(),
                record["rkey"]
                    .as_str()
                    .ok_or("Missing record key")?
                    .to_string(),
            );
            let mut unsigned = record.clone();
            let sig = unsigned
                .as_object_mut()
                .and_then(|v| v.remove("sig"))
                .and_then(|v| v.as_str().map(str::to_string))
                .ok_or("Missing signature")?;
            let sig: [u8; 64] = hex::decode(sig)
                .map_err(err)?
                .try_into()
                .map_err(|_| "Invalid signature")?;
            let cid = record::signed_record_cid(&unsigned, &sig)
                .map_err(err)?
                .to_string();
            if engine.stored.iter().any(|r| r.did == owner && r.cid == cid) {
                continue;
            }
            let _ = key;
            engine.ingest_record(record).map_err(err)?;
        }
        engine.persist().map_err(err)
    }
}

impl ArchiveStore {
    /// Cache only host-custodied Tangled signing material, encrypted under the host key.
    /// Foreign service credentials are never accepted by this store.
    pub fn remember_signer(&self, owner: &str, id: &Identity) -> Result<(), String> {
        let capsule = seal(
            &recipient(&self.master).agreement_public_bytes(),
            owner.as_bytes(),
            &encode(&id.to_seeds())?,
        )?;
        self.db
            .execute(
                "INSERT OR REPLACE INTO signers VALUES(?1,?2)",
                params![owner, capsule.to_string()],
            )
            .map_err(err)?;
        Ok(())
    }
    pub fn connector_signer(&self, owner: &str) -> Result<Identity, String> {
        let capsule: String = self
            .db
            .query_row("SELECT capsule FROM signers WHERE owner=?1", [owner], |r| {
                r.get(0)
            })
            .map_err(err)?;
        let (ed, x): ([u8; 32], [u8; 32]) = serde_json::from_slice(&open(
            &self.master,
            owner.as_bytes(),
            &serde_json::from_str::<Value>(&capsule).map_err(err)?,
        )?)
        .map_err(err)?;
        Ok(Identity::from_seeds(ed, x))
    }
}

impl ArchiveStore {
    /// Validate every actually retained file, including unrendered source files.
    /// A foreign attachment reference can legitimately have no retained bytes;
    /// a manifest for retained bytes must always be complete and decryptable.
    pub fn verify_archive(&self, owner: &str, id: &Identity) -> Result<(), String> {
        if !self.has_archive(owner)? {
            return Ok(());
        }
        let key = self.archive_key(owner, id)?;
        for rec in self.values(owner, FILE)? {
            let data: Value = Self::plaintext(&key, &rec)?;
            let manifest: FileManifest =
                serde_json::from_value(data["file"].clone()).map_err(err)?;
            self.read_file(owner, &key, &manifest)?;
        }
        for rec in self.values(owner, BATCH)? {
            let batch: SavedBatch = Self::plaintext(&key, &rec)?;
            self.read_file(owner, &key, &batch.original)?;
        }
        self.objects(owner, id)?;
        Ok(())
    }
}

impl ArchiveStore {
    pub fn files(&self, owner: &str, id: &Identity) -> Result<Vec<ArchiveFileMetadata>, String> {
        if !self.has_archive(owner)? {
            return Ok(vec![]);
        }
        let key = self.archive_key(owner, id)?;
        self.values(owner, FILE)?
            .iter()
            .map(|record| {
                let value: Value = Self::plaintext(&key, record)?;
                let source = value["source"]
                    .as_str()
                    .ok_or("Invalid file source")?
                    .to_string();
                let account = value["account"]
                    .as_str()
                    .ok_or("Invalid file account")?
                    .to_string();
                let file: FileManifest =
                    serde_json::from_value(value["file"].clone()).map_err(err)?;
                let logical = opaque(
                    &key,
                    &encode(&(source.as_str(), account.as_str(), file.name.as_str()))?,
                );
                Ok(ArchiveFileMetadata {
                    key: logical,
                    source,
                    account,
                    name: file.name,
                    media_type: file.media_type,
                })
            })
            .collect()
    }
}
/// X post/event snowflakes and relationship target IDs are source-global.
/// Conversation is enrichment, and must not produce a different object/tombstone.
fn identity_conversation<'a>(source: &str, object: &'a ImportedObject) -> Option<&'a str> {
    if source == "twitter"
        && matches!(
            object.kind.as_str(),
            "post" | "message" | "like" | "bookmark" | "following" | "follower"
        )
    {
        None
    } else {
        object.conversation.as_deref()
    }
}

/// Resolve missing media placeholders without treating raw source content as a
/// new revision. Existing content-addressed attachments may never be replaced.
fn missing_media_enrichment(prior: &ImportedObject, next: &ImportedObject) -> bool {
    fn source_name(alias: &str) -> Option<&str> {
        let (hash, name) = alias.split_once('/')?;
        (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(name)
    }
    prior.id == next.id
        && prior.kind == next.kind
        && prior.conversation == next.conversation
        && prior.sender == next.sender
        && prior.timestamp == next.timestamp
        && prior.text == next.text
        && prior.data == next.data
        && prior.attachments != next.attachments
        && prior.attachments.iter().all(|old| {
            next.attachments.iter().any(|new| {
                old == new || (source_name(old).is_none() && source_name(new) == Some(old.as_str()))
            })
        })
}

/// Compare native revisions across archive and connector representations. Dates
/// without a UTC offset are deliberately not assigned an invented timezone.
fn source_revision(source: &str, object: &ImportedObject) -> Option<i128> {
    fn timestamp(value: &Value) -> Option<i128> {
        if let Some(seconds) = value
            .as_i64()
            .or_else(|| value.as_str()?.parse::<i64>().ok())
        {
            return Some(i128::from(seconds) * 1_000_000_000);
        }
        time::OffsetDateTime::parse(
            value.as_str()?,
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
        .map(|time| time.unix_timestamp_nanos())
    }
    if let Some(revision) = object.data.get("source_revision").and_then(timestamp) {
        return Some(revision);
    }
    if source == "telegram" {
        // Telegram Desktop emits edited_unixtime/date_unixtime, whereas
        // Telethon serializes edit_date/date as offset-qualified ISO strings.
        for field in ["edited_unixtime", "edit_date", "date_unixtime", "date"] {
            if let Some(revision) = object.data.get(field).and_then(timestamp) {
                return Some(revision);
            }
        }
    }
    object
        .timestamp
        .as_ref()
        .and_then(|value| timestamp(&Value::String(value.clone())))
}

pub fn validate_batch(batch: &ImportBatch) -> Result<(), String> {
    if !["telegram", "twitter", "whatsapp"].contains(&batch.source.as_str())
        || batch.account.trim().is_empty()
        || batch.account.len() > 256
    {
        return Err("Unsupported source or missing stable source account ID".into());
    }
    if batch.objects.len() > 100_000 || batch.files.len() > 10_000 {
        return Err("Split this import into smaller batches".into());
    }
    if batch
        .objects
        .iter()
        .any(|o| o.id.is_empty() || o.id.len() > 4096 || o.kind.is_empty())
    {
        return Err("Source objects need stable IDs and kinds".into());
    }
    Ok(())
}
