//! Seed a running knot with public records so Weft's explorer is not empty.
//!
//! Path matches Reed: SIWS `putRecord` of `vis: public` posts. Process
//! orchestration (three knots + `--peer`) lives in `just weft-seed`.

use std::path::{Path, PathBuf};

use tangled::http::{KnotClient, PutRecordRequest};
use tangled::session::{sign_siws, SiwsVerifyRequest};
use tangled::Identity;

use crate::error::{Result, WeftError};
use crate::types::KnotUrl;
use crate::Explorer;

/// Same layout as `apps/weft/src/paths.rs`.
pub fn weft_home() -> PathBuf {
    if let Ok(p) = std::env::var("WEFT_HOME") {
        return PathBuf::from(p);
    }
    if let Ok(h) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(h).join(".weft");
    }
    PathBuf::from(".weft")
}

pub fn knot_path() -> PathBuf {
    knot_path_in(&weft_home())
}

pub fn knot_path_in(home: &Path) -> PathBuf {
    home.join("knot.txt")
}

pub fn persist_knot_if_missing(path: &Path, url: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, url.trim())?;
    Ok(())
}

pub struct SeedOpts {
    pub knot: String,
    pub knot_path: PathBuf,
}

#[derive(Debug)]
pub struct SeedReport {
    pub knot: String,
    pub identities: Vec<String>,
    pub records: usize,
}

pub async fn siws_client_on(client: KnotClient, identity: &Identity) -> Result<KnotClient> {
    let did = identity.did_string();
    let ch = client.session_challenge(&did).await?;
    let sig = sign_siws(identity.signing_key(), &ch.message);
    let (_, token) = client
        .session_verify(&SiwsVerifyRequest {
            did,
            nonce: ch.nonce,
            signature: sig,
        })
        .await?;
    Ok(client.with_session_cookie(token))
}

pub async fn siws_client(url: &str, identity: &Identity) -> Result<KnotClient> {
    siws_client_on(KnotClient::http(url), identity).await
}

fn unix_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

fn unsigned_post(author: &str, rkey: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "v": 1,
        "type": tangled::Cid::dag_cbor(b"pub.tangled.post").to_string(),
        "collection": "pub.tangled.post",
        "rkey": rkey,
        "author": author,
        "ts": unix_us(),
        "prev": [],
        "vis": "public",
        "body": { "text": text },
    })
}

/// Sign and put `texts` as public posts via a SIWS client.
pub async fn publish_public(
    identity: &Identity,
    client: &KnotClient,
    texts: &[&str],
) -> Result<usize> {
    let author = identity.did_string();
    let mut n = 0;
    for (i, text) in texts.iter().enumerate() {
        let unsigned = unsigned_post(&author, &format!("weft-{i}"), text);
        let signed = tangled::record::sign_json_record(identity.signing_key(), &unsigned)?;
        client
            .put_record(&PutRecordRequest {
                record: signed.json,
            })
            .await?;
        n += 1;
    }
    Ok(n)
}

pub async fn run(opts: SeedOpts) -> Result<SeedReport> {
    let ex = Explorer::http(KnotUrl(opts.knot.clone()))?;
    ex.local_stats()
        .await
        .map_err(|_| WeftError::KnotUnreachable)?;

    persist_knot_if_missing(&opts.knot_path, &opts.knot)?;

    let ada = Identity::generate();
    let bob = Identity::generate();

    let ada_client = siws_client(&opts.knot, &ada).await?;
    let ada_n = publish_public(&ada, &ada_client, &["ada on the mesh", "second from ada"]).await?;

    let bob_client = siws_client(&opts.knot, &bob).await?;
    let bob_n = publish_public(&bob, &bob_client, &["bob on the mesh"]).await?;

    Ok(SeedReport {
        knot: opts.knot,
        identities: vec![ada.did_string(), bob.did_string()],
        records: ada_n + bob_n,
    })
}
