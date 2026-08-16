use crate::error::Result;
use tangled::http::{KnotClient, PutRecordRequest};

#[derive(Default)]
pub struct Outbox {
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

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub async fn drain(&mut self, client: &KnotClient) -> Result<usize> {
        let mut n = 0;
        while !self.pending.is_empty() {
            let rec = self.pending[0].clone();
            client.put_record(&PutRecordRequest { record: rec }).await?;
            self.pending.remove(0);
            n += 1;
        }
        Ok(n)
    }
}
