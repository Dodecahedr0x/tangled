//! Shared HTTP test harness: one identity + one listening knot per node.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;

use tangled::capability::{cap_authorization_header, Capability, Invocation};
use tangled::http::{KnotClient, PutRecordRequest, PutRecordResponse};
use tangled::identity::Identity;
use tangled::record::{sign_json_record, SignedRecord};
use tangled::{Cid, Error, KnotEngine, Result};
use tangled_knot::bind_ephemeral;

pub const NOW: u64 = 1_700_000_000;

pub struct Live {
    pub identity: Identity,
    pub addr: SocketAddr,
    pub now: u64,
    pub state: tangled_knot::AppState,
}

impl Live {
    pub async fn named(name: &str) -> Self {
        Self::spawn(name, None, &[]).await
    }

    pub async fn with_data(name: &str, dir: PathBuf) -> Self {
        Self::spawn(name, Some(dir), &[]).await
    }

    pub async fn with_peers(name: &str, peers: &[&str]) -> Self {
        let peers: Vec<String> = peers.iter().map(|s| (*s).to_string()).collect();
        Self::spawn(name, None, &peers).await
    }

    async fn spawn(name: &str, data: Option<PathBuf>, peers: &[String]) -> Self {
        let identity = identity_named(name);
        let did = identity.did_string();
        let mut engine = if let Some(dir) = data {
            KnotEngine::open(&dir, &did).expect("open engine")
        } else {
            KnotEngine::hosting(&did)
        };
        engine.now = NOW;
        engine.register_identity(&identity);
        engine.set_public_peers(peers);
        let (addr, state) = bind_ephemeral(engine, false).await.expect("bind");
        Self {
            identity,
            addr,
            now: NOW,
            state,
        }
    }

    pub fn did(&self) -> String {
        self.identity.did_string()
    }

    pub fn seed_actor(&self, doc: serde_json::Value) {
        self.state.ap.seed_actor(doc);
    }

    pub fn remote_inbox(&self, actor: &str) -> Option<String> {
        self.state.ap.remote_inbox(actor)
    }

    pub fn client(&self) -> KnotClient {
        KnotClient::http(format!("http://{}", self.addr))
    }

    pub fn client_cap(&self, header: String) -> KnotClient {
        self.client().with_cap_header(header)
    }

    pub async fn siws_token(&self) -> String {
        siws_token(&self.client(), &self.identity)
            .await
            .expect("siws")
    }

    pub fn issue_cap(
        &self,
        aud: &str,
        cmd: &str,
        res: &str,
    ) -> Result<(Capability, serde_json::Value)> {
        let mut cap = Capability {
            v: 1,
            iss: self.did(),
            aud: aud.to_string(),
            cmd: cmd.to_string(),
            res: res.to_string(),
            nbf: self.now,
            exp: self.now + 86_400,
            pol: vec![],
            prf: vec![],
            meta: None,
            sig: None,
        };
        cap.sign(self.identity.signing_key())?;
        let mut json = cap.to_unsigned_json();
        json["sig"] = serde_json::Value::String(hex::encode(cap.sig.expect("signed")));
        Ok((cap, json))
    }

    pub fn invocation_header(
        &self,
        cap: &Capability,
        cmd: &str,
        res: &str,
        knot_did: &str,
    ) -> String {
        let mut inv = Invocation::new(cap, cmd, res, knot_did, self.now, [7u8; 16]).expect("inv");
        inv.sign(self.identity.signing_key()).expect("sign inv");
        cap_authorization_header(&inv.encode_cbor().expect("cbor"))
    }

    pub fn unsigned(
        &self,
        collection: &str,
        rkey: &str,
        vis: &str,
        body: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "v": 1,
            "type": schema_cid(collection),
            "collection": collection,
            "rkey": rkey,
            "author": self.did(),
            "ts": self.now * 1000,
            "prev": [],
            "vis": vis,
            "body": body,
        })
    }

    pub fn sign(&self, unsigned: &serde_json::Value) -> Result<SignedRecord> {
        sign_json_record(self.identity.signing_key(), unsigned)
    }

    pub async fn publish(&self, unsigned: serde_json::Value) -> Result<PublishOut> {
        let signed = self.sign(&unsigned)?;
        let put = self
            .client()
            .put_record(&PutRecordRequest {
                record: signed.json.clone(),
            })
            .await?;
        Ok(PublishOut {
            cid: signed.cid,
            put,
        })
    }
}

#[derive(Debug)]
pub struct PublishOut {
    pub cid: Cid,
    pub put: PutRecordResponse,
}

pub fn identity_named(name: &str) -> Identity {
    let ed = *blake3::hash(name.as_bytes()).as_bytes();
    let x = *blake3::hash(format!("{name}/x25519").as_bytes()).as_bytes();
    Identity::from_seeds(ed, x)
}

pub fn schema_cid(collection: &str) -> String {
    Cid::dag_cbor(collection.as_bytes()).to_string()
}

pub fn http_code(err: &Error) -> (u16, String) {
    match err {
        Error::Http { status, error } => (*status, error.clone()),
        other => panic!("expected HTTP error, got {other}"),
    }
}

pub async fn siws_token(client: &KnotClient, identity: &Identity) -> Result<String> {
    let ch = client.session_challenge(&identity.did_string()).await?;
    let sig = tangled::session::sign_siws(identity.signing_key(), &ch.message);
    let (_, token) = client
        .session_verify(&tangled::session::SiwsVerifyRequest {
            did: identity.did_string(),
            nonce: ch.nonce,
            signature: sig,
        })
        .await?;
    Ok(token)
}
