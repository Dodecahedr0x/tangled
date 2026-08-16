use crate::error::Result;
use tangled::http::{KnotClient, PutRecordRequest};

#[derive(Default)]
pub struct Outbox {
    pending_blobs: Vec<Vec<u8>>,
    pending: Vec<serde_json::Value>,
}

impl Outbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue_record(&mut self, record: serde_json::Value) -> Result<()> {
        self.pending.push(record);
        Ok(())
    }

    pub fn enqueue_blob(&mut self, bytes: Vec<u8>) -> Result<()> {
        self.pending_blobs.push(bytes);
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.pending_blobs.is_empty()
    }

    pub async fn drain(&mut self, client: &KnotClient) -> Result<usize> {
        let mut n = 0;
        while !self.pending_blobs.is_empty() {
            let bytes = self.pending_blobs.remove(0);
            client.put_blob(bytes).await?;
            n += 1;
        }
        while !self.pending.is_empty() {
            let rec = self.pending[0].clone();
            client.put_record(&PutRecordRequest { record: rec }).await?;
            self.pending.remove(0);
            n += 1;
        }
        Ok(n)
    }
}
