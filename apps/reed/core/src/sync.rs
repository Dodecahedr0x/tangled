use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tangled::dagcbor::{self, Value};
use tangled::http::KnotClient;
use tangled::MockKnot;

use crate::error::{ReedError, Result};
use crate::identity::ReedKeys;
use crate::replica::Replica;
use crate::types::KnotUrl;

pub struct Sync {
    pub knot: KnotUrl,
    pub client: KnotClient,
    remotes: HashMap<String, Arc<Mutex<MockKnot>>>,
}

impl Sync {
    pub fn http(knot: KnotUrl) -> Result<Self> {
        if knot.0.is_empty() {
            return Err(ReedError::KnotUnreachable);
        }
        Ok(Self {
            client: KnotClient::http(knot.0.clone()),
            knot,
            remotes: HashMap::new(),
        })
    }

    pub fn mock(engine: Arc<Mutex<MockKnot>>) -> Self {
        Self {
            knot: KnotUrl("mock://".into()),
            client: KnotClient::mock_shared(engine),
            remotes: HashMap::new(),
        }
    }

    pub fn with_remote(mut self, url: impl Into<String>, engine: Arc<Mutex<MockKnot>>) -> Self {
        self.remotes.insert(url.into(), engine);
        self
    }

    pub async fn probe(&self) -> Result<()> {
        self.client
            .describe()
            .await
            .map(|_| ())
            .map_err(|_| ReedError::KnotUnreachable)
    }

    pub async fn poll_once(&mut self, replica: &mut Replica, keys: &ReedKeys) -> Result<()> {
        let me = keys.did().as_str();
        if let Ok(d) = self.client.describe().await {
            replica.set_max_blob(d.max_blob);
        }
        ingest_public(&self.client, replica, &me).await?;
        for (subject, knot) in replica.follow_knots(&me) {
            if subject == me {
                continue;
            }
            match knot
                .as_deref()
                .filter(|u| !u.is_empty() && !same_knot(u, &self.knot.0))
            {
                Some(url) => {
                    let client = self.client_for(url);
                    ingest_public(&client, replica, &subject).await?;
                }
                None => ingest_public(&self.client, replica, &subject).await?,
            }
        }
        Ok(())
    }

    fn client_for(&self, url: &str) -> KnotClient {
        let key = url.trim_end_matches('/');
        if let Some(engine) = self
            .remotes
            .iter()
            .find(|(k, _)| k.trim_end_matches('/') == key)
            .map(|(_, e)| e.clone())
        {
            KnotClient::mock_shared(engine)
        } else {
            KnotClient::http(url)
        }
    }
}

fn same_knot(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
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
        let bytes = serde_json::to_vec(&rec).map_err(|e| ReedError::Codec(e.to_string()))?;
        replica.apply_record(&bytes)?;
    }
    pull_missing_blobs(client, replica, did).await;
    Ok(())
}

async fn pull_missing_blobs(client: &KnotClient, replica: &mut Replica, did: &str) {
    let mut cids = Vec::new();
    if let Some(cid) = replica.profile(did).and_then(|p| p.avatar_cid) {
        cids.push(cid);
    }
    for post in replica.profile_posts(did) {
        for media in post.media.iter().take(4) {
            cids.push(media.cid.clone());
        }
    }
    for cid in cids {
        if replica.blob(&cid).is_some() {
            continue;
        }
        if let Ok(bytes) = client.get_blob(did, &cid).await {
            let _ = replica.store_blob(&cid, &bytes);
        }
    }
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

fn map_poll(e: tangled::Error) -> ReedError {
    match e {
        tangled::Error::Transport(_) => ReedError::KnotUnreachable,
        tangled::Error::Http { .. } => ReedError::KnotUnreachable,
        other => ReedError::Protocol(other),
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
