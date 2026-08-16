use std::sync::{Arc, Mutex};

use rand::RngCore;
use tangled::capability::{cap_authorization_header, circle_resource_uri, Invocation};
use tangled::dagcbor::{self, Value};
use tangled::http::{GetRepoAuthRequest, KnotClient};
use tangled::Capability;
use tangled::MockKnot;

use crate::chat::Chat;
use crate::error::{LoomedError, Result};
use crate::identity::LoomedKeys;
use crate::receipts::{is_receipt, open_receipt, RECEIPT_COLLECTION};
use crate::replica::Replica;
use crate::types::{CircleRef, KnotUrl};

pub struct Sync {
    pub knot: KnotUrl,
    pub client: KnotClient,
    mock: Option<Arc<Mutex<MockKnot>>>,
}

impl Sync {
    pub fn http(knot: KnotUrl) -> Result<Self> {
        if knot.0.is_empty() {
            return Err(LoomedError::KnotUnreachable);
        }
        Ok(Self {
            client: KnotClient::http(knot.0.clone()),
            knot,
            mock: None,
        })
    }

    pub fn mock(engine: Arc<Mutex<MockKnot>>) -> Self {
        Self {
            knot: KnotUrl("mock://".into()),
            client: KnotClient::mock_shared(engine.clone()),
            mock: Some(engine),
        }
    }

    pub async fn probe(&self) -> Result<()> {
        self.client
            .describe()
            .await
            .map(|_| ())
            .map_err(|_| LoomedError::KnotUnreachable)
    }

    pub async fn poll_once(&mut self, replica: &mut Replica, keys: &LoomedKeys) -> Result<()> {
        let me = keys.did().as_str();
        let knot_did = match self.client.describe().await {
            Ok(d) => d.did,
            Err(_) => me.clone(),
        };
        let mut dids = vec![knot_did.clone(), me.clone()];
        // Shared-knot inbox: public sys.circle records that name us (members).
        if let Ok(q) = self.client.loom_query(&me, Some("sys.circle")).await {
            for hit in q.hits {
                if !dids.contains(&hit.did) {
                    dids.push(hit.did);
                }
            }
        }
        dids.sort();
        dids.dedup();
        for did in &dids {
            ingest_public(&self.client, replica, did).await?;
        }
        let chat = Chat { keys: keys.clone() };
        let circles = replica.conversations();
        for conv in circles {
            let Some((owner, rkey)) = conv.circle.0.split_once('#') else {
                continue;
            };
            let owner = owner.to_string();
            let rkey = rkey.to_string();
            let res = circle_resource_uri(&owner, &rkey);
            let cap = match circle_read_cap(keys, replica, &owner, &res) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let header = match invocation_header(keys, &cap, "circle/read", &res, &knot_did) {
                Ok(h) => h,
                Err(_) => continue,
            };
            let authed = self.client_with_cap(header);
            let car_bytes = match authed
                .get_repo_auth(&GetRepoAuthRequest {
                    did: owner.clone(),
                    since: None,
                    blobs: Some(false),
                })
                .await
            {
                Ok(b) => b,
                Err(_) => continue,
            };
            let recs = records_from_car(&car_bytes)?;
            apply_wraps(replica, keys, &recs);
            apply_sealed(&chat, replica, keys, &recs);
        }
        Ok(())
    }

    fn client_with_cap(&self, header: String) -> KnotClient {
        if let Some(engine) = &self.mock {
            KnotClient::mock_shared(engine.clone()).with_cap_header(header)
        } else {
            KnotClient::http(self.knot.0.clone()).with_cap_header(header)
        }
    }
}

async fn ingest_public(client: &KnotClient, replica: &mut Replica, did: &str) -> Result<()> {
    match client.get_head(did).await {
        Ok(_) => {}
        Err(tangled::Error::Http { status: 404, .. }) => return Ok(()),
        Err(e) => return Err(map_poll(e)),
    }
    let car_bytes = match client.get_repo(did, None, Some(false)).await {
        Ok(b) => b,
        Err(tangled::Error::Http { status: 404, .. }) => return Ok(()),
        Err(e) => return Err(map_poll(e)),
    };
    let recs = records_from_car(&car_bytes)?;
    for rec in recs {
        if rec.get("collection").and_then(|c| c.as_str()) == Some("sys.circle") {
            let bytes = serde_json::to_vec(&rec).map_err(|e| LoomedError::Codec(e.to_string()))?;
            replica.apply_record(&bytes)?;
        }
    }
    Ok(())
}

fn records_from_car(car_bytes: &[u8]) -> Result<Vec<serde_json::Value>> {
    if car_bytes.is_empty() {
        return Ok(Vec::new());
    }
    let car = tangled::car::decode(car_bytes).map_err(map_poll)?;
    let mut recs = Vec::new();
    for (_cid, data) in car.blocks {
        if let Ok(val) = dagcbor::decode(&data) {
            let json = cbor_to_json(&val);
            if json.get("collection").is_some() {
                recs.push(json);
            }
        }
    }
    Ok(recs)
}

fn apply_wraps(replica: &mut Replica, keys: &LoomedKeys, recs: &[serde_json::Value]) {
    let sk = keys.identity().agreement_secret().to_bytes();
    for rec in recs {
        if rec.get("collection").and_then(|c| c.as_str()) != Some("sys.circle.wrap") {
            continue;
        }
        let Some(circle) = rec.get("circle").and_then(|c| c.as_str()) else {
            continue;
        };
        let body = rec.get("body").cloned().unwrap_or(serde_json::Value::Null);
        let Some(enc_hex) = body.get("enc").and_then(|e| e.as_str()) else {
            continue;
        };
        let Some(ct_hex) = body.get("ct").and_then(|c| c.as_str()) else {
            continue;
        };
        let epoch = body
            .get("epoch")
            .and_then(|e| e.as_u64())
            .or_else(|| rec.get("epoch").and_then(|e| e.as_u64()))
            .unwrap_or(0);
        let Ok(enc) = hex::decode(enc_hex) else {
            continue;
        };
        let Ok(ct) = hex::decode(ct_hex) else {
            continue;
        };
        if let Ok(secret) = tangled::hpke::unwrap_epoch_secret(&sk, &enc, circle, epoch, &ct) {
            replica.remember_secret(circle, epoch, secret);
        }
    }
}

fn apply_sealed(chat: &Chat, replica: &mut Replica, keys: &LoomedKeys, recs: &[serde_json::Value]) {
    for rec in recs {
        let collection = rec.get("collection").and_then(|c| c.as_str()).unwrap_or("");
        let Some(circle_s) = rec.get("circle").and_then(|c| c.as_str()) else {
            continue;
        };
        let circle = CircleRef(circle_s.to_string());
        let Some(state) = replica.circle_state(&circle) else {
            continue;
        };
        if collection == "pub.tangled.chat" {
            if let Ok(msg) = chat.open_message(&state, rec) {
                let _ = replica.apply_record(&serde_json::to_vec(&msg).unwrap_or_default());
            }
        } else if collection == RECEIPT_COLLECTION || is_receipt(rec) {
            if let Ok(opened) = open_receipt(keys, &state, rec) {
                let _ = replica.apply_record(&serde_json::to_vec(&opened).unwrap_or_default());
            } else {
                let _ = replica.apply_record(&serde_json::to_vec(rec).unwrap_or_default());
            }
        }
    }
}

fn circle_read_cap(
    keys: &LoomedKeys,
    replica: &Replica,
    owner: &str,
    res: &str,
) -> Result<Capability> {
    let me = keys.did().as_str();
    if let Some(cap) = replica.grant_for(&me, res) {
        return Ok(cap);
    }
    if me == owner {
        return issue_circle_read(keys, &me, res);
    }
    Err(LoomedError::Identity("no circle/read grant".into()))
}

fn issue_circle_read(keys: &LoomedKeys, aud: &str, res: &str) -> Result<Capability> {
    let now = unix_secs();
    let iss = keys.did().as_str();
    let mut cap = Capability {
        v: 1,
        iss: iss.clone(),
        aud: aud.to_string(),
        cmd: "circle/read".into(),
        res: res.to_string(),
        nbf: now.saturating_sub(60),
        exp: now + 86_400,
        pol: vec![],
        prf: vec![],
        meta: None,
        sig: None,
    };
    let id = keys.identity();
    cap.sign(id.signing_key())?;
    Ok(cap)
}

fn invocation_header(
    keys: &LoomedKeys,
    cap: &Capability,
    cmd: &str,
    res: &str,
    knot_did: &str,
) -> Result<String> {
    let now = unix_secs();
    let mut nonce = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce);
    let mut inv = Invocation::new(cap, cmd, res, knot_did, now, nonce)?;
    let id = keys.identity();
    inv.sign(id.signing_key())?;
    Ok(cap_authorization_header(&inv.encode_cbor()?))
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn map_poll(e: tangled::Error) -> LoomedError {
    match e {
        tangled::Error::Transport(_) => LoomedError::KnotUnreachable,
        tangled::Error::Http { .. } => LoomedError::KnotUnreachable,
        other => LoomedError::Protocol(other),
    }
}

fn cbor_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Unsigned(n) => serde_json::json!(*n),
        Value::Negative(n) => serde_json::json!(*n),
        Value::Bytes(b) => serde_json::Value::String(hex::encode(b)),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Array(a) => serde_json::Value::Array(a.iter().map(cbor_to_json).collect()),
        Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in m {
                obj.insert(k.clone(), cbor_to_json(val));
            }
            serde_json::Value::Object(obj)
        }
        Value::Cid(c) => serde_json::Value::String(c.to_string()),
    }
}
