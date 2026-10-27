//! Encrypted durable staging for resumable source archives.
use super::*;
use sha2::{Digest, Sha256};
pub const UPLOAD_CHUNK: usize = 1024 * 1024;
const UPLOAD_TTL: u64 = 7 * 24 * 60 * 60 * 1000;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadRequest {
    pub source: String,
    pub account: String,
    pub filename: String,
    pub total: usize,
    #[serde(default)]
    pub sha256: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadStatus {
    pub id: String,
    pub source: String,
    pub account: String,
    pub filename: String,
    pub total: usize,
    pub received: usize,
    pub status: String,
    pub chunk_size: usize,
    pub updated_at: u64,
    pub receipt: Option<Receipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct UploadMetadata {
    request: UploadRequest,
    key: String,
    receipt: Option<Receipt>,
    error: Option<String>,
}
impl ArchiveStore {
    fn upload_tables(&self) -> Result<(), String> {
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS upload_sessions(id TEXT PRIMARY KEY,owner TEXT NOT NULL,source TEXT NOT NULL,account TEXT NOT NULL,generation INTEGER NOT NULL,metadata TEXT NOT NULL,received INTEGER NOT NULL,total INTEGER NOT NULL,status TEXT NOT NULL,updated_at INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS upload_chunks(id TEXT NOT NULL,offset INTEGER NOT NULL,capsule TEXT NOT NULL,PRIMARY KEY(id,offset));").map_err(err)
    }
    fn upload_aad(owner: &str, id: &str, offset: Option<usize>) -> Result<Vec<u8>, String> {
        encode(&("tangled.import.upload.v1", owner, id, offset))
    }
    fn upload_metadata(
        &self,
        owner: &str,
        id: &str,
    ) -> Result<(UploadMetadata, UploadStatus, u64), String> {
        self.upload_tables()?;
        let (capsule,received,total,status,generation,updated_at):(String,usize,usize,String,u64,u64)=self.db.query_row(
            "SELECT metadata,received,total,status,generation,updated_at FROM upload_sessions WHERE id=?1 AND owner=?2",params![id,owner],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(|_|"Upload not found in this account".to_string())?;
        if now().saturating_sub(updated_at) > UPLOAD_TTL && status != "complete" {
            return Err("Upload expired. Start this file again.".into());
        }
        let metadata: UploadMetadata = serde_json::from_slice(&open(
            &self.master,
            &Self::upload_aad(owner, id, None)?,
            &serde_json::from_str::<Value>(&capsule).map_err(err)?,
        )?)
        .map_err(err)?;
        let info = UploadStatus {
            id: id.into(),
            source: metadata.request.source.clone(),
            account: metadata.request.account.clone(),
            filename: metadata.request.filename.clone(),
            total,
            received,
            status,
            chunk_size: UPLOAD_CHUNK,
            updated_at,
            receipt: metadata.receipt.clone(),
        };
        Ok((metadata, info, generation))
    }
    pub fn create_upload(
        &mut self,
        owner: &str,
        request: UploadRequest,
    ) -> Result<UploadStatus, String> {
        self.upload_tables()?;
        if !["telegram", "twitter", "whatsapp"].contains(&request.source.as_str())
            || request.account.is_empty()
            || request.account.len() > 256
            || request.filename.is_empty()
            || request.filename.len() > 1024
            || request.total == 0
            || request.total > MAX_UPLOAD
        {
            return Err(
                "Choose a supported source, source account, and file of 1 byte to 256 MiB".into(),
            );
        }
        if request
            .sha256
            .as_ref()
            .is_some_and(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid SHA-256 file digest".into());
        }
        // Bound dormant staging even if an owner opens many abandoned uploads.
        self.expire_uploads()?;
        let pending:usize=self.db.query_row("SELECT COUNT(*) FROM upload_sessions WHERE owner=?1 AND status NOT IN ('complete','canceled')",[owner],|r|r.get(0)).map_err(err)?;
        if pending >= 8 {
            return Err("Cancel or finish an existing upload before starting another".into());
        }
        let id = hex::encode(random_key());
        let metadata = UploadMetadata {
            request: request.clone(),
            key: hex::encode(random_key()),
            receipt: None,
            error: None,
        };
        let capsule = seal(
            &recipient(&self.master).agreement_public_bytes(),
            &Self::upload_aad(owner, &id, None)?,
            &encode(&metadata)?,
        )?;
        let generation = self.erasure_generation(owner)?;
        self.db
            .execute(
                "INSERT INTO upload_sessions VALUES(?1,?2,?3,?4,?5,?6,0,?7,'uploading',?8)",
                params![
                    id,
                    owner,
                    request.source,
                    opaque(
                        &self.master,
                        &encode(&(owner, request.source.as_str(), request.account.as_str()))?
                    ),
                    generation,
                    capsule.to_string(),
                    request.total,
                    now()
                ],
            )
            .map_err(err)?;
        self.upload_status(owner, &id)
    }
    pub fn upload_status(&self, owner: &str, id: &str) -> Result<UploadStatus, String> {
        let (_, status, generation) = self.upload_metadata(owner, id)?;
        self.check_erasure_generation(owner, generation)?;
        Ok(status)
    }
    pub fn append_upload(
        &mut self,
        owner: &str,
        id: &str,
        offset: usize,
        bytes: &[u8],
    ) -> Result<UploadStatus, String> {
        let (metadata, mut status, generation) = self.upload_metadata(owner, id)?;
        self.check_erasure_generation(owner, generation)?;
        if status.status != "uploading" && status.status != "ready" {
            return Err("Upload is not accepting chunks".into());
        }
        if bytes.is_empty()
            || bytes.len() > UPLOAD_CHUNK
            || offset.saturating_add(bytes.len()) > status.total
        {
            return Err("Chunk exceeds the upload's remaining size or 1 MiB limit".into());
        }
        let key = decode_key(&metadata.key)?;
        if offset < status.received {
            let capsule: Option<String> = self
                .db
                .query_row(
                    "SELECT capsule FROM upload_chunks WHERE id=?1 AND offset=?2",
                    params![id, offset],
                    |r| r.get(0),
                )
                .optional()
                .map_err(err)?;
            let same = if let Some(capsule) = capsule {
                open(
                    &key,
                    &Self::upload_aad(owner, id, Some(offset))?,
                    &serde_json::from_str::<Value>(&capsule).map_err(err)?,
                )? == bytes
            } else {
                false
            };
            return if same {
                Ok(status)
            } else {
                Err("This offset was already uploaded with different bytes".into())
            };
        }
        if offset != status.received {
            return Err(format!(
                "Upload offset conflict: resume at {}",
                status.received
            ));
        }
        let capsule = seal(
            &recipient(&key).agreement_public_bytes(),
            &Self::upload_aad(owner, id, Some(offset))?,
            bytes,
        )?;
        let received = offset + bytes.len();
        status.status = if received == status.total {
            "ready"
        } else {
            "uploading"
        }
        .into();
        let tx = self.db.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO upload_chunks VALUES(?1,?2,?3)",
            params![id, offset, capsule.to_string()],
        )
        .map_err(err)?;
        tx.execute("UPDATE upload_sessions SET received=?1,status=?2,updated_at=?3 WHERE id=?4 AND owner=?5",params![received,status.status,now(),id,owner]).map_err(err)?;
        tx.commit().map_err(err)?;
        self.upload_status(owner, id)
    }
    pub fn upload_bytes(&self, owner: &str, id: &str) -> Result<(UploadStatus, Vec<u8>), String> {
        let (metadata, status, generation) = self.upload_metadata(owner, id)?;
        self.check_erasure_generation(owner, generation)?;
        if status.status != "ready" {
            return Err("Upload is not complete or has been canceled".into());
        }
        let key = decode_key(&metadata.key)?;
        let mut stmt = self
            .db
            .prepare("SELECT offset,capsule FROM upload_chunks WHERE id=?1 ORDER BY offset")
            .map_err(err)?;
        let chunks = stmt
            .query_map([id], |r| {
                Ok((r.get::<_, usize>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(err)?;
        let mut bytes = Vec::with_capacity(status.total);
        for chunk in chunks {
            let (offset, capsule) = chunk.map_err(err)?;
            if offset != bytes.len() {
                return Err("Upload staging has a gap; restart this file".into());
            }
            bytes.extend(open(
                &key,
                &Self::upload_aad(owner, id, Some(offset))?,
                &serde_json::from_str::<Value>(&capsule).map_err(err)?,
            )?);
        }
        if bytes.len() != status.total {
            return Err("Uploaded byte count does not match the file".into());
        }
        if metadata
            .request
            .sha256
            .as_ref()
            .is_some_and(|hash| hex::encode(Sha256::digest(&bytes)) != hash.to_ascii_lowercase())
        {
            return Err(
                "The file digest did not match. Cancel and upload the original file again.".into(),
            );
        }
        Ok((status, bytes))
    }
    pub fn finish_upload(
        &mut self,
        owner: &str,
        id: &str,
        receipt: &Receipt,
    ) -> Result<UploadStatus, String> {
        let (mut metadata, status, generation) = self.upload_metadata(owner, id)?;
        self.check_erasure_generation(owner, generation)?;
        if status.status == "complete" {
            return Ok(status);
        }
        if status.status != "ready" {
            return Err("Upload is not ready".into());
        }
        metadata.receipt = Some(receipt.clone());
        let capsule = seal(
            &recipient(&self.master).agreement_public_bytes(),
            &Self::upload_aad(owner, id, None)?,
            &encode(&metadata)?,
        )?;
        let tx = self.db.transaction().map_err(err)?;
        tx.execute("DELETE FROM upload_chunks WHERE id=?1", [id])
            .map_err(err)?;
        tx.execute("UPDATE upload_sessions SET metadata=?1,status='complete',updated_at=?2 WHERE id=?3 AND owner=?4",params![capsule.to_string(),now(),id,owner]).map_err(err)?;
        tx.commit().map_err(err)?;
        self.upload_status(owner, id)
    }
    pub fn cancel_upload(&mut self, owner: &str, id: &str) -> Result<(), String> {
        let (_, status, _) = self.upload_metadata(owner, id)?;
        if status.status == "complete" {
            return Err("This upload is already imported; erase its source data instead".into());
        }
        let tx = self.db.transaction().map_err(err)?;
        tx.execute("DELETE FROM upload_chunks WHERE id=?1", [id])
            .map_err(err)?;
        tx.execute("UPDATE upload_sessions SET status='canceled',received=0,updated_at=?1 WHERE id=?2 AND owner=?3",params![now(),id,owner]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
    pub fn expire_uploads(&mut self) -> Result<(), String> {
        self.upload_tables()?;
        let expiry = now().saturating_sub(UPLOAD_TTL);
        let tx = self.db.transaction().map_err(err)?;
        tx.execute("DELETE FROM upload_chunks WHERE id IN (SELECT id FROM upload_sessions WHERE updated_at<?1)",[expiry]).map_err(err)?;
        tx.execute("DELETE FROM upload_sessions WHERE updated_at<?1", [expiry])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }
}
