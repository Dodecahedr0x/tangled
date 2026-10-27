//! Multi-node in-process harness: one identity + knot store per node.
//! Knots never share a database; clients pull via `KnotClient`.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use tangled::capability::{cap_authorization_header, Capability, Invocation};
use tangled::cid::Cid;
use tangled::http::{KnotClient, MockKnot, PutRecordRequest, PutRecordResponse};
use tangled::identity::Identity;
use tangled::record::{sign_json_record, SignedRecord};
use tangled::{Error, Result};

/// Injected unix seconds used for capability windows and record timestamps.
pub const NOW: u64 = 1_700_000_000;

pub struct Node {
    pub identity: Identity,
    pub store: Arc<Mutex<MockKnot>>,
    pub now: u64,
}

pub struct PublishOut {
    pub cid: Cid,
    pub put: PutRecordResponse,
}

impl Node {
    pub fn named(name: &str) -> Self {
        let identity = identity_named(name);
        let did = identity.did_string();
        let mut knot = MockKnot::hosting(&did);
        knot.register_identity(&identity);
        Self {
            identity,
            store: Arc::new(Mutex::new(knot)),
            now: NOW,
        }
    }

    pub fn did(&self) -> String {
        self.identity.did_string()
    }

    pub fn agreement_pk(&self) -> [u8; 32] {
        self.identity.agreement_public_bytes()
    }

    /// Client bound to this node's knot (publisher / local replica).
    pub fn local(&self) -> KnotClient {
        KnotClient::mock_shared(self.store.clone())
    }

    /// Client that talks to `other`'s knot (pull). No shared store.
    pub fn to(&self, other: &Node) -> KnotClient {
        KnotClient::mock_shared(other.store.clone())
    }

    pub fn to_with_cap(&self, other: &Node, header: String) -> KnotClient {
        KnotClient::mock_shared(other.store.clone()).with_cap_header(header)
    }

    pub async fn siws_on(&self, other: &Node) -> Result<String> {
        siws_token(&self.to(other), &self.identity).await
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
            .local()
            .put_record(&PutRecordRequest {
                record: signed.json.clone(),
            })
            .await?;
        Ok(PublishOut {
            cid: signed.cid,
            put,
        })
    }

    #[allow(dead_code)]
    pub async fn publish_signed(&self, json: serde_json::Value) -> Result<PutRecordResponse> {
        self.local()
            .put_record(&PutRecordRequest { record: json })
            .await
    }

    /// Store a verified record as replica ingest (no ConcurrentHeads / write-cap).
    pub fn ingest(&self, json: serde_json::Value) -> Result<PutRecordResponse> {
        self.store.lock().expect("knot").ingest_record(json)
    }
}

/// Deterministic identity (blake3 of the label). Ed25519 ≠ X25519 seeds.
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

pub fn load_vector(name: &str) -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/spec/vectors")
        .join(name);
    let s = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_json::from_str(&s).expect("vector json")
}

pub fn page_from_list_item(item: &tangled::http::ListRecordItem) -> tangled::page::PageRecord {
    let mut unsigned = item.record.clone();
    if let Some(obj) = unsigned.as_object_mut() {
        obj.remove("sig");
    }
    tangled::page::record_from_unsigned(&item.cid, &unsigned).expect("page record")
}
