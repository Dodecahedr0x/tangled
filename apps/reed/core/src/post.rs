use crate::error::{ReedError, Result};
use crate::identity::ReedKeys;
use crate::outbox::Outbox;
use crate::replica::Replica;
use crate::types::PostId;
use crate::wire::{put_signed, sign, unsigned, MAX_MEDIA, MAX_TEXT, MEDIA, POST};

pub struct PostApi {
    pub keys: ReedKeys,
}

impl PostApi {
    pub fn compose_text(
        &self,
        replica: &mut Replica,
        outbox: &mut Outbox,
        text: &str,
        reply_to: Option<&PostId>,
        quote_of: Option<&PostId>,
    ) -> Result<PostId> {
        self.compose_with_media(replica, outbox, text, &[], reply_to, quote_of)
    }

    pub fn compose_with_media(
        &self,
        replica: &mut Replica,
        outbox: &mut Outbox,
        text: &str,
        blobs: &[(Vec<u8>, String, Option<String>)],
        reply_to: Option<&PostId>,
        quote_of: Option<&PostId>,
    ) -> Result<PostId> {
        if text.len() > MAX_TEXT {
            return Err(ReedError::Codec("text exceeds 32 KiB".into()));
        }
        if blobs.len() > MAX_MEDIA {
            return Err(ReedError::MediaLimit);
        }
        let max_blob = replica.max_blob() as usize;
        for (bytes, _, _) in blobs {
            if bytes.len() > max_blob {
                return Err(ReedError::MediaTooLarge);
            }
        }
        let author = self.keys.did().as_str();
        let mut media_cids = Vec::new();
        for (bytes, mime, alt) in blobs {
            let blob_cid = tangled::Cid::raw(bytes).to_string();
            replica.store_blob(&blob_cid, bytes)?;
            outbox.enqueue_blob(bytes.clone())?;
            let rkey = tangled::identity::tid_now();
            let mut body = serde_json::json!({
                "blob": blob_cid,
                "mime": mime,
            });
            if let Some(a) = alt {
                body["alt"] = serde_json::Value::String(a.clone());
            }
            let rec = sign(
                &self.keys,
                &unsigned(&author, MEDIA, &rkey, "public", body, None),
            )?;
            media_cids.push(put_signed(replica, outbox, rec)?);
        }
        let mut body = serde_json::json!({ "text": text });
        if let Some(parent) = reply_to {
            if quote_of.is_none() {
                body["reply"] = serde_json::Value::String(parent.0.clone());
            }
        }
        if !media_cids.is_empty() {
            body["media"] = serde_json::Value::Array(
                media_cids
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            );
        }
        let rkey = tangled::identity::tid_now();
        let env_ref = quote_of.map(|q| q.0.as_str());
        let rec = sign(
            &self.keys,
            &unsigned(&author, POST, &rkey, "public", body, env_ref),
        )?;
        Ok(PostId(put_signed(replica, outbox, rec)?))
    }

    pub fn edit_text(
        &self,
        replica: &mut Replica,
        outbox: &mut Outbox,
        id: &PostId,
        text: &str,
    ) -> Result<()> {
        if text.len() > MAX_TEXT {
            return Err(ReedError::Codec("text exceeds 32 KiB".into()));
        }
        let post = replica
            .post(id)
            .ok_or_else(|| ReedError::Codec("post not found".into()))?;
        if post.author != self.keys.did().as_str() {
            return Err(ReedError::Identity("not author".into()));
        }
        let mut body = replica
            .post_body(id)
            .ok_or_else(|| ReedError::Codec("post not found".into()))?;
        body["text"] = serde_json::Value::String(text.to_string());
        let env_ref = post.quote_of.as_ref().map(|q| q.0.as_str());
        let rec = sign(
            &self.keys,
            &unsigned(&post.author, POST, &post.rkey, "public", body, env_ref),
        )?;
        put_signed(replica, outbox, rec)?;
        Ok(())
    }

    pub fn delete(&self, replica: &mut Replica, outbox: &mut Outbox, id: &PostId) -> Result<()> {
        let post = replica
            .post(id)
            .ok_or_else(|| ReedError::Codec("post not found".into()))?;
        if post.author != self.keys.did().as_str() {
            return Err(ReedError::Identity("not author".into()));
        }
        let body = serde_json::json!({ "text": "", "deleted": true });
        let rec = sign(
            &self.keys,
            &unsigned(&post.author, POST, &post.rkey, "public", body, None),
        )?;
        put_signed(replica, outbox, rec)?;
        Ok(())
    }
}
