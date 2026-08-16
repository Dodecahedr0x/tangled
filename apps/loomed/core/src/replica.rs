use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tangled::circle::CircleState;
use tangled::Capability;

use crate::error::{LoomedError, Result};
use crate::receipts::RECEIPT_COLLECTION;
use crate::types::{CircleRef, Conversation, Message, MessageId};

#[derive(Serialize, Deserialize, Clone)]
struct CircleMeta {
    circle: String,
    owner: String,
    rkey: String,
    name: String,
    members: Vec<String>,
    epoch: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct StoredSecret {
    circle: String,
    epoch: u64,
    secret_hex: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Data {
    conversations: Vec<Conversation>,
    messages: Vec<Message>,
    grants: Vec<serde_json::Value>,
    circles: Vec<CircleMeta>,
    secrets: Vec<StoredSecret>,
}

pub struct Replica {
    dir: PathBuf,
    data: Data,
}

impl Replica {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("replica.json");
        let data = if path.exists() {
            let s = std::fs::read_to_string(&path)?;
            serde_json::from_str(&s).map_err(|e| LoomedError::Codec(e.to_string()))?
        } else {
            Data::default()
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            data,
        })
    }

    pub fn conversations(&self) -> Vec<Conversation> {
        self.data
            .conversations
            .iter()
            .cloned()
            .map(|mut c| {
                if c.members.is_empty() {
                    if let Some(meta) = self.data.circles.iter().find(|m| m.circle == c.circle.0) {
                        c.members = meta.members.clone();
                    }
                }
                c
            })
            .collect()
    }

    pub fn messages(&self, circle: &CircleRef) -> Vec<Message> {
        self.data
            .messages
            .iter()
            .filter(|m| m.circle == *circle)
            .cloned()
            .collect()
    }

    pub fn apply_record(&mut self, bytes: &[u8]) -> Result<()> {
        let v: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| LoomedError::Codec(e.to_string()))?;
        if v.get("ts_us").is_some() && v.get("id").is_some() {
            let msg: Message =
                serde_json::from_value(v).map_err(|e| LoomedError::Codec(e.to_string()))?;
            self.upsert_message(msg);
            self.persist()?;
            return Ok(());
        }
        let collection = v.get("collection").and_then(|c| c.as_str()).unwrap_or("");
        match collection {
            "sys.circle" => {
                let author = v
                    .get("author")
                    .and_then(|a| a.as_str())
                    .ok_or_else(|| LoomedError::Codec("sys.circle missing author".into()))?;
                let rkey = v
                    .get("rkey")
                    .and_then(|r| r.as_str())
                    .ok_or_else(|| LoomedError::Codec("sys.circle missing rkey".into()))?;
                let body = v.get("body").cloned().unwrap_or(serde_json::Value::Null);
                let title = body
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("chat")
                    .to_string();
                let circle = format!("{author}#{rkey}");
                let members = body
                    .get("members")
                    .and_then(|m| m.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![author.to_string()]);
                self.upsert_conversation(Conversation {
                    circle: CircleRef(circle.clone()),
                    title: title.clone(),
                    last_preview: String::new(),
                    unread: 0,
                    members: members.clone(),
                });
                let epoch = body.get("epoch").and_then(|e| e.as_u64()).unwrap_or(0);
                self.upsert_meta(CircleMeta {
                    circle: circle.clone(),
                    owner: author.to_string(),
                    rkey: rkey.to_string(),
                    name: title,
                    members,
                    epoch,
                });
                if let Some(grants) = body.get("grants").and_then(|g| g.as_array()) {
                    for g in grants {
                        self.remember_grant(g.clone());
                    }
                }
                for e in 0..=epoch {
                    let known = self
                        .data
                        .secrets
                        .iter()
                        .any(|s| s.circle == circle && s.epoch == e);
                    if !known {
                        let mut secret = [0u8; 32];
                        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut secret);
                        self.remember_secret(&circle, e, secret);
                    }
                }
            }
            c if c == RECEIPT_COLLECTION => {
                let author = v.get("author").and_then(|a| a.as_str());
                let target = v.get("ref").and_then(|r| r.as_str());
                if let (Some(author), Some(target)) = (author, target) {
                    if let Some(msg) = self
                        .data
                        .messages
                        .iter_mut()
                        .find(|m| m.id == MessageId(target.to_string()))
                    {
                        if !msg.receipts.iter().any(|d| d == author) {
                            msg.receipts.push(author.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        self.persist()?;
        Ok(())
    }

    pub(crate) fn remember_grant(&mut self, grant: serde_json::Value) {
        if !self.data.grants.iter().any(|g| g == &grant) {
            self.data.grants.push(grant);
        }
    }

    pub(crate) fn remember_secret(&mut self, circle: &str, epoch: u64, secret: [u8; 32]) {
        if let Some(s) = self
            .data
            .secrets
            .iter_mut()
            .find(|s| s.circle == circle && s.epoch == epoch)
        {
            s.secret_hex = hex::encode(secret);
        } else {
            self.data.secrets.push(StoredSecret {
                circle: circle.to_string(),
                epoch,
                secret_hex: hex::encode(secret),
            });
        }
    }

    pub(crate) fn grant_for(&self, aud: &str, res: &str) -> Option<Capability> {
        self.data.grants.iter().find_map(|g| {
            let cap = Capability::from_json(g).ok()?;
            if cap.aud == aud && cap.res == res && cap.cmd == "circle/read" {
                Some(cap)
            } else {
                None
            }
        })
    }

    pub fn circle_state(&self, circle: &CircleRef) -> Option<CircleState> {
        let meta = self.data.circles.iter().find(|c| c.circle == circle.0)?;
        let mut secrets = std::collections::BTreeMap::new();
        for s in &self.data.secrets {
            if s.circle != circle.0 {
                continue;
            }
            if let Ok(bytes) = hex::decode(&s.secret_hex) {
                if let Ok(arr) = <[u8; 32]>::try_from(bytes) {
                    secrets.insert(s.epoch, arr);
                }
            }
        }
        let secret0 = secrets.values().next().copied().unwrap_or([0u8; 32]);
        let mut st = CircleState::create(
            meta.owner.clone(),
            meta.rkey.clone(),
            meta.name.clone(),
            secret0,
        );
        st.members = meta.members.clone();
        st.epoch = meta.epoch;
        for (ep, sec) in secrets {
            st.epoch_secrets.insert(ep, sec);
        }
        Some(st)
    }

    fn upsert_meta(&mut self, meta: CircleMeta) {
        if let Some(c) = self
            .data
            .circles
            .iter_mut()
            .find(|c| c.circle == meta.circle)
        {
            *c = meta;
        } else {
            self.data.circles.push(meta);
        }
    }

    fn upsert_message(&mut self, msg: Message) {
        let preview = msg.text.clone().unwrap_or_else(|| {
            if msg.blob_cid.is_some() {
                "[image]".into()
            } else {
                String::new()
            }
        });
        if let Some(c) = self
            .data
            .conversations
            .iter_mut()
            .find(|c| c.circle == msg.circle)
        {
            c.last_preview = preview;
        } else {
            self.data.conversations.push(Conversation {
                circle: msg.circle.clone(),
                title: "chat".into(),
                last_preview: preview,
                unread: 0,
                members: self
                    .data
                    .circles
                    .iter()
                    .find(|m| m.circle == msg.circle.0)
                    .map(|m| m.members.clone())
                    .unwrap_or_default(),
            });
        }
        if let Some(existing) = self.data.messages.iter_mut().find(|m| m.id == msg.id) {
            let receipts = existing.receipts.clone();
            *existing = msg;
            for r in receipts {
                if !existing.receipts.contains(&r) {
                    existing.receipts.push(r);
                }
            }
        } else {
            self.data.messages.push(msg);
        }
    }

    fn upsert_conversation(&mut self, conv: Conversation) {
        if let Some(c) = self
            .data
            .conversations
            .iter_mut()
            .find(|c| c.circle == conv.circle)
        {
            c.title = conv.title;
            if !conv.members.is_empty() {
                c.members = conv.members;
            }
            if c.last_preview.is_empty() {
                c.last_preview = conv.last_preview;
            }
        } else {
            self.data.conversations.push(conv);
        }
    }

    fn persist(&self) -> Result<()> {
        let path = self.dir.join("replica.json");
        let s = serde_json::to_string(&self.data).map_err(|e| LoomedError::Codec(e.to_string()))?;
        std::fs::write(path, s)?;
        Ok(())
    }
}
