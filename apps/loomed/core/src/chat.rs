use crate::envelope::{open_plaintext, seal_json, sign, signed_cid, unsigned};
use crate::error::{LoomedError, Result};
use crate::identity::LoomedKeys;
use crate::outbox::Outbox;
use crate::replica::Replica;
use crate::types::{CircleRef, Message, MessageId};
use rand::RngCore;
use tangled::circle::CircleState;
use tangled::constants::CIRCLE_MLS_THRESHOLD;
use tangled::Did;

pub struct Chat {
    pub keys: LoomedKeys,
}

impl Chat {
    pub fn create_circle(&self, members: &[Did]) -> Result<(CircleRef, serde_json::Value)> {
        let owner = self.keys.did();
        let owner_s = owner.as_str();
        let rkey = tangled::identity::tid_now();
        let circle = CircleRef(format!("{owner_s}#{rkey}"));
        let mut member_ids = vec![owner_s.clone()];
        for m in members {
            let s = m.as_str();
            if s != owner_s {
                member_ids.push(s);
            }
        }
        let name = if member_ids.len() <= 2 {
            "chat"
        } else {
            "group"
        };
        let mut secret0 = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret0);
        let mut st = CircleState::create(owner_s.clone(), rkey.clone(), name.to_string(), secret0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        for m in member_ids.iter().skip(1) {
            let mut sec = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut sec);
            st.add_member(m.clone(), sec, now)?;
        }
        let cres = tangled::capability::circle_resource_uri(&owner_s, &rkey);
        let id = self.keys.identity();
        let mut grants = Vec::new();
        for m in &st.members {
            let mut cap = tangled::Capability {
                v: 1,
                iss: owner_s.clone(),
                aud: m.clone(),
                cmd: "circle/read".into(),
                res: cres.clone(),
                nbf: now.saturating_sub(60),
                exp: now + 86_400,
                pol: vec![],
                prf: vec![],
                meta: None,
                sig: None,
            };
            cap.sign(id.signing_key())?;
            let mut json = cap.to_unsigned_json();
            if let Some(sig) = cap.sig {
                json["sig"] = serde_json::Value::String(hex::encode(sig));
            }
            grants.push(json);
        }
        let body = serde_json::json!({
            "v": 1,
            "name": name,
            "members": st.members,
            "policy": {
                "invite": "owner",
                "write": "member",
                "history_share": false,
                "hideMembers": false
            },
            "epoch": st.epoch,
            "distributor": st.distributor.as_str(),
            "threshold": CIRCLE_MLS_THRESHOLD,
            "grants": grants,
        });
        let rec = unsigned(&owner_s, "sys.circle", &rkey, "public", body, None, None);
        Ok((circle, sign(&self.keys, &rec)?))
    }

    pub fn seal_text(
        &self,
        circle: &CircleRef,
        circle_state: &CircleState,
        text: &str,
    ) -> Result<serde_json::Value> {
        seal_json(
            &self.keys,
            circle,
            circle_state,
            "pub.tangled.chat",
            serde_json::json!({ "circle": circle.0, "text": text }),
            None,
        )
    }

    pub fn seal_image(
        &self,
        circle: &CircleRef,
        circle_state: &CircleState,
        blob_cid: &str,
    ) -> Result<serde_json::Value> {
        seal_json(
            &self.keys,
            circle,
            circle_state,
            "pub.tangled.chat",
            serde_json::json!({ "circle": circle.0, "blob": blob_cid }),
            None,
        )
    }

    pub fn open_message(
        &self,
        circle_state: &CircleState,
        record: &serde_json::Value,
    ) -> Result<Message> {
        let pt = open_plaintext(circle_state, record)?;
        let circle = record
            .get("circle")
            .and_then(|c| c.as_str())
            .or_else(|| pt.get("circle").and_then(|c| c.as_str()))
            .ok_or_else(|| LoomedError::Codec("missing circle".into()))?;
        let author = record
            .get("author")
            .and_then(|a| a.as_str())
            .ok_or_else(|| LoomedError::Codec("missing author".into()))?;
        let ts_us = record.get("ts").and_then(|t| t.as_u64()).unwrap_or(0);
        Ok(Message {
            id: MessageId(signed_cid(record)?),
            circle: CircleRef(circle.to_string()),
            author: author.to_string(),
            text: pt.get("text").and_then(|t| t.as_str()).map(str::to_string),
            blob_cid: pt.get("blob").and_then(|t| t.as_str()).map(str::to_string),
            ts_us,
            receipts: Vec::new(),
        })
    }

    pub fn send_text(
        &self,
        replica: &mut Replica,
        outbox: &mut Outbox,
        circle: &CircleRef,
        text: &str,
    ) -> Result<MessageId> {
        self.send_sealed(replica, outbox, circle, |s, st| {
            s.seal_text(circle, st, text)
        })
    }

    pub fn send_image(
        &self,
        replica: &mut Replica,
        outbox: &mut Outbox,
        circle: &CircleRef,
        blob_cid: &str,
    ) -> Result<MessageId> {
        self.send_sealed(replica, outbox, circle, |s, st| {
            s.seal_image(circle, st, blob_cid)
        })
    }

    fn send_sealed(
        &self,
        replica: &mut Replica,
        outbox: &mut Outbox,
        circle: &CircleRef,
        seal: impl FnOnce(&Self, &CircleState) -> Result<serde_json::Value>,
    ) -> Result<MessageId> {
        let state = replica
            .circle_state(circle)
            .ok_or_else(|| LoomedError::Codec("unknown circle".into()))?;
        let signed = seal(self, &state)?;
        let opened = self.open_message(&state, &signed)?;
        let id = opened.id.clone();
        replica.apply_record(
            &serde_json::to_vec(&opened).map_err(|e| LoomedError::Codec(e.to_string()))?,
        )?;
        outbox.enqueue_record(signed)?;
        Ok(id)
    }
}
