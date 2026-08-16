//! Seed a running knot with inbound DMs so Loomed's inbox is not empty.
//!
//! Path matches `sealed_poll` / `knot_live`: a peer owns the circle, publishes
//! `sys.circle` + `sys.circle.wrap` for the local member, SIWS `putRecord`,
//! then the local app's Refresh / `poll_once` decrypts.

use std::path::{Path, PathBuf};

use tangled::circle::CircleState;
use tangled::http::KnotClient;
use tangled::session::{sign_siws, SiwsVerifyRequest};

use crate::chat::Chat;
use crate::error::{LoomedError, Result};
use crate::identity::{FileKeyStore, KeyStore, LoomedKeys};
use crate::outbox::Outbox;
use crate::replica::Replica;
use crate::sync::Sync;
use crate::types::{CircleRef, KnotUrl};

/// Same layout as `apps/loomed/src/paths.rs`.
pub fn loomed_home() -> PathBuf {
    if let Ok(p) = std::env::var("LOOMED_HOME") {
        return PathBuf::from(p);
    }
    if let Ok(h) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(h).join(".loomed");
    }
    PathBuf::from(".loomed")
}

pub fn keys_path() -> PathBuf {
    keys_path_in(&loomed_home())
}

pub fn keys_path_in(home: &Path) -> PathBuf {
    home.join("keys.json")
}

pub struct SeedOpts {
    pub knot: String,
    pub keys_path: PathBuf,
    pub create_local: bool,
}

#[derive(Debug)]
pub struct SeedReport {
    pub local_did: String,
    pub peers: Vec<String>,
    pub circles: usize,
    pub messages: usize,
}

pub fn load_local_keys(path: &Path, create: bool) -> Result<LoomedKeys> {
    let store = FileKeyStore {
        path: path.to_path_buf(),
    };
    if let Some(keys) = store.load()? {
        return Ok(keys);
    }
    if create {
        let keys = LoomedKeys::generate();
        store.save(&keys)?;
        return Ok(keys);
    }
    Err(LoomedError::Identity(format!(
        "no Loomed identity at {}; generate one in Loomed first, or pass --create-local",
        path.display()
    )))
}

pub fn wrap_for_member(
    owner: &LoomedKeys,
    member: &LoomedKeys,
    circle: &str,
    state: &CircleState,
) -> Result<serde_json::Value> {
    let secret = *state
        .epoch_secrets
        .get(&state.epoch)
        .ok_or_else(|| LoomedError::Codec("epoch secret on owner replica".into()))?;
    let (enc, ct) = tangled::hpke::wrap_epoch_secret(
        &member.identity().agreement_public_bytes(),
        circle,
        state.epoch,
        &secret,
    )?;
    let rkey = format!(
        "{}-{}",
        state.epoch,
        tangled::identity::encode_pubkey_base58(&member.identity().pubkey_bytes())
    );
    let unsigned = serde_json::json!({
        "v": 1,
        "type": tangled::Cid::dag_cbor(b"sys.circle.wrap").to_string(),
        "collection": "sys.circle.wrap",
        "rkey": rkey,
        "author": owner.did().as_str(),
        "ts": unix_us(),
        "prev": [],
        "vis": "sealed",
        "circle": circle,
        "epoch": state.epoch,
        "body": {
            "alg": "hpke-record",
            "enc": hex::encode(enc),
            "ct": hex::encode(ct),
            "epoch": state.epoch,
        }
    });
    Ok(tangled::record::sign_json_record(owner.identity().signing_key(), &unsigned)?.json)
}

pub async fn siws_client_on(client: KnotClient, keys: &LoomedKeys) -> Result<KnotClient> {
    let did = keys.did().as_str();
    let ch = client.session_challenge(&did).await?;
    let sig = sign_siws(keys.identity().signing_key(), &ch.message);
    let (_, token) = client
        .session_verify(&SiwsVerifyRequest {
            did,
            nonce: ch.nonce,
            signature: sig,
        })
        .await?;
    Ok(client.with_session_cookie(token))
}

pub async fn siws_client(url: &str, keys: &LoomedKeys) -> Result<KnotClient> {
    siws_client_on(KnotClient::http(url), keys).await
}

/// Peer owns the circle, wraps every `members` key and the owner, sends `texts`, drains via SIWS client.
pub async fn open_and_send(
    owner: &LoomedKeys,
    members: &[LoomedKeys],
    texts: &[&str],
    client: &KnotClient,
) -> Result<(CircleRef, usize)> {
    let chat = Chat {
        keys: owner.clone(),
    };
    let dids: Vec<_> = members.iter().map(|m| m.did()).collect();
    let (circle, sys) = chat.create_circle(&dids)?;

    let dir = std::env::temp_dir().join(format!(
        "loomed-seed-{}-{}",
        std::process::id(),
        owner.did().as_str().replace(':', "_")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir)?;
    replica
        .apply_record(&serde_json::to_vec(&sys).map_err(|e| LoomedError::Codec(e.to_string()))?)?;

    let mut outbox = Outbox::new();
    outbox.enqueue_record(sys)?;
    let state = replica
        .circle_state(&circle)
        .ok_or_else(|| LoomedError::Codec("circle_state after sys.circle".into()))?;
    for member in members {
        outbox.enqueue_record(wrap_for_member(owner, member, &circle.0, &state)?)?;
    }
    outbox.enqueue_record(wrap_for_member(owner, owner, &circle.0, &state)?)?;
    for text in texts {
        chat.send_text(&mut replica, &mut outbox, &circle, text)?;
    }
    let n = outbox.drain(client).await?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok((circle, n))
}

pub async fn run(opts: SeedOpts) -> Result<SeedReport> {
    let sync = Sync::http(KnotUrl(opts.knot.clone()))?;
    sync.probe().await?;

    let local = load_local_keys(&opts.keys_path, opts.create_local)?;
    let bob = LoomedKeys::generate();
    let carol = LoomedKeys::generate();

    let bob_client = siws_client(&opts.knot, &bob).await?;
    let (_, bob_n) = open_and_send(
        &bob,
        std::slice::from_ref(&local),
        &[
            "hey — is this thing on?",
            "bob here. your inbox should not be empty.",
            "third from bob",
        ],
        &bob_client,
    )
    .await?;

    let carol_client = siws_client(&opts.knot, &carol).await?;
    let (_, carol_n) = open_and_send(
        &carol,
        std::slice::from_ref(&local),
        &["carol checking in", "also from carol"],
        &carol_client,
    )
    .await?;

    let (_, group_n) = open_and_send(
        &bob,
        &[local.clone(), carol.clone()],
        &["welcome to the group", "three of us in here"],
        &bob_client,
    )
    .await?;

    let local_client = siws_client(&opts.knot, &local).await?;
    let (_, local_n) = open_and_send(
        &local,
        std::slice::from_ref(&bob),
        &["thanks, got it"],
        &local_client,
    )
    .await?;

    Ok(SeedReport {
        local_did: local.did().as_str(),
        peers: vec![bob.did().as_str(), carol.did().as_str()],
        circles: 4,
        messages: bob_n + carol_n + group_n + local_n,
    })
}

fn unix_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}
