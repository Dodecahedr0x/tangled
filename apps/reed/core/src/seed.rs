//! Seed a running knot with public posts and a follow graph so Reed is not empty.
//!
//! Path matches `poll_once`: peers publish `pub.tangled.post` and
//! `tangled.graph.follow` via SIWS `putRecord`, then the local app's Refresh
//! / `poll_once` ingests them.

use std::path::{Path, PathBuf};

use tangled::http::KnotClient;
use tangled::session::{sign_siws, SiwsVerifyRequest};
use tangled::Did;

use crate::error::{ReedError, Result};
use crate::graph::follow;
use crate::identity::{FileKeyStore, KeyStore, ReedKeys};
use crate::outbox::Outbox;
use crate::post::PostApi;
use crate::replica::Replica;
use crate::social::{boost, like, put_profile};
use crate::sync::Sync;
use crate::types::{KnotUrl, PostId, Profile};

/// Same layout as `apps/reed/src/paths.rs`.
pub fn reed_home() -> PathBuf {
    if let Ok(p) = std::env::var("REED_HOME") {
        return PathBuf::from(p);
    }
    if let Ok(h) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(h).join(".reed");
    }
    PathBuf::from(".reed")
}

pub fn keys_path() -> PathBuf {
    keys_path_in(&reed_home())
}

pub fn keys_path_in(home: &Path) -> PathBuf {
    home.join("keys.json")
}

pub fn knot_path_in(home: &Path) -> PathBuf {
    home.join("knot.txt")
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
    pub posts: usize,
    pub records: usize,
}

pub fn load_local_keys(path: &Path, create: bool) -> Result<ReedKeys> {
    let store = FileKeyStore {
        path: path.to_path_buf(),
    };
    if let Some(keys) = store.load()? {
        return Ok(keys);
    }
    if create {
        let keys = ReedKeys::generate();
        store.save(&keys)?;
        return Ok(keys);
    }
    Err(ReedError::Identity(format!(
        "no Reed identity at {}; generate one in Reed first, or pass --create-local",
        path.display()
    )))
}

fn persist_knot_if_missing(path: &Path, url: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, url.trim())?;
    Ok(())
}

pub async fn siws_client_on(client: KnotClient, keys: &ReedKeys) -> Result<KnotClient> {
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

pub async fn siws_client(url: &str, keys: &ReedKeys) -> Result<KnotClient> {
    siws_client_on(KnotClient::http(url), keys).await
}

fn seed_dir(keys: &ReedKeys) -> PathBuf {
    std::env::temp_dir().join(format!(
        "reed-seed-{}-{}",
        std::process::id(),
        keys.did().as_str().replace(':', "_")
    ))
}

/// Compose `texts` and follow `subjects`, then drain via a SIWS client.
pub async fn publish_posts_and_follows(
    keys: &ReedKeys,
    client: &KnotClient,
    texts: &[&str],
    follow_dids: &[Did],
    follow_knot: Option<&str>,
) -> Result<(Vec<PostId>, usize)> {
    let dir = seed_dir(keys);
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir)?;
    let mut outbox = Outbox::new();
    let api = PostApi { keys: keys.clone() };
    let mut ids = Vec::new();
    for text in texts {
        ids.push(api.compose_text(&mut replica, &mut outbox, text, None, None)?);
    }
    for subject in follow_dids {
        follow(keys, &mut replica, &mut outbox, subject, follow_knot)?;
    }
    let n = outbox.drain(client).await?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok((ids, n))
}

struct Engagement<'a> {
    display_name: &'a str,
    bio: &'a str,
    reply: Option<(&'a PostId, &'a str)>,
    quote: Option<(&'a PostId, &'a str)>,
    like: Option<&'a PostId>,
    boost: Option<&'a PostId>,
}

async fn publish_engagement(
    keys: &ReedKeys,
    client: &KnotClient,
    e: Engagement<'_>,
) -> Result<(Vec<PostId>, usize)> {
    let dir = seed_dir(keys);
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir)?;
    let mut outbox = Outbox::new();
    let api = PostApi { keys: keys.clone() };
    put_profile(
        keys,
        &mut replica,
        &mut outbox,
        &Profile {
            did: keys.did().as_str(),
            handle: None,
            display_name: Some(e.display_name.into()),
            bio: Some(e.bio.into()),
            avatar_cid: None,
            source_url: None,
        },
    )?;
    let mut ids = Vec::new();
    if let Some((parent, text)) = e.reply {
        ids.push(api.compose_text(&mut replica, &mut outbox, text, Some(parent), None)?);
    }
    if let Some((quoted, text)) = e.quote {
        ids.push(api.compose_text(&mut replica, &mut outbox, text, None, Some(quoted))?);
    }
    if let Some(target) = e.like {
        like(keys, &mut replica, &mut outbox, target)?;
    }
    if let Some(target) = e.boost {
        boost(keys, &mut replica, &mut outbox, target)?;
    }
    let n = outbox.drain(client).await?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok((ids, n))
}

pub async fn run(opts: SeedOpts) -> Result<SeedReport> {
    let sync = Sync::http(KnotUrl(opts.knot.clone()))?;
    sync.probe().await?;

    let local = load_local_keys(&opts.keys_path, opts.create_local)?;
    if let Some(home) = opts.keys_path.parent() {
        persist_knot_if_missing(&knot_path_in(home), &opts.knot)?;
    }

    let ada = ReedKeys::generate();
    let bob = ReedKeys::generate();
    let carol = ReedKeys::generate();
    let knot = Some(opts.knot.as_str());

    let ada_client = siws_client(&opts.knot, &ada).await?;
    let (ada_posts, ada_n) = publish_posts_and_follows(
        &ada,
        &ada_client,
        &["the square is open — say hello", "second from ada"],
        std::slice::from_ref(&local.did()),
        knot,
    )
    .await?;
    let ada_op = ada_posts
        .first()
        .ok_or_else(|| ReedError::Codec("ada opener missing".into()))?;
    let (_, ada_e) = publish_engagement(
        &ada,
        &ada_client,
        Engagement {
            display_name: "Ada",
            bio: "wires the square",
            reply: None,
            quote: None,
            like: None,
            boost: None,
        },
    )
    .await?;

    let bob_client = siws_client(&opts.knot, &bob).await?;
    let (bob_posts, bob_n) = publish_posts_and_follows(
        &bob,
        &bob_client,
        &["bob here. home should not be empty."],
        &[ada.did(), local.did()],
        knot,
    )
    .await?;
    let (_, bob_e) = publish_engagement(
        &bob,
        &bob_client,
        Engagement {
            display_name: "Bob",
            bio: "checking the timeline",
            reply: Some((ada_op, "hey — is this thing on?")),
            quote: None,
            like: Some(ada_op),
            boost: None,
        },
    )
    .await?;

    let carol_client = siws_client(&opts.knot, &carol).await?;
    let (carol_posts, carol_n) = publish_posts_and_follows(
        &carol,
        &carol_client,
        &["carol checking in"],
        &[ada.did(), local.did()],
        knot,
    )
    .await?;
    let (_, carol_e) = publish_engagement(
        &carol,
        &carol_client,
        Engagement {
            display_name: "Carol",
            bio: "discover should not be empty",
            reply: None,
            quote: Some((ada_op, "quoting the opener")),
            like: None,
            boost: Some(ada_op),
        },
    )
    .await?;

    let local_client = siws_client(&opts.knot, &local).await?;
    let (local_posts, local_n) = publish_posts_and_follows(
        &local,
        &local_client,
        &["thanks, got it"],
        &[ada.did(), bob.did(), carol.did()],
        knot,
    )
    .await?;
    let (_, local_e) = publish_engagement(
        &local,
        &local_client,
        Engagement {
            display_name: "You",
            bio: "reed demo",
            reply: None,
            quote: None,
            like: None,
            boost: None,
        },
    )
    .await?;

    Ok(SeedReport {
        local_did: local.did().as_str(),
        peers: vec![ada.did().as_str(), bob.did().as_str(), carol.did().as_str()],
        posts: ada_posts.len() + bob_posts.len() + carol_posts.len() + local_posts.len() + 2,
        records: ada_n + ada_e + bob_n + bob_e + carol_n + carol_e + local_n + local_e,
    })
}

/// Follow a remote ActivityPub actor after `tangled.ap.resolve` (outbox pull).
#[derive(Debug)]
pub struct ApSeedReport {
    pub local_did: String,
    pub actor: String,
    pub acct: String,
    pub notes: usize,
    pub records: usize,
}

/// Default public Mastodon account for `just reed-ap-seed`. Override with
/// `REED_AP_ACCT` or `reed-ap-seed --acct`.
pub const DEFAULT_AP_ACCT: &str = "acct:Gargron@mastodon.social";

pub fn default_ap_acct() -> String {
    std::env::var("REED_AP_ACCT").unwrap_or_else(|_| DEFAULT_AP_ACCT.into())
}

#[cfg(not(target_arch = "wasm32"))]
const AP_DEMO_NOTES: &[&str] = &[
    "hello from the mock mastodon — this Note is federated",
    "second toot: Home and Discover fill after Refresh",
];

/// Resolve `acct`, pull the public outbox, follow from the Reed identity.
pub async fn run_ap(opts: SeedOpts) -> Result<ApSeedReport> {
    run_ap_acct(opts, default_ap_acct()).await
}

pub async fn run_ap_acct(opts: SeedOpts, acct: String) -> Result<ApSeedReport> {
    let sync = Sync::http(KnotUrl(opts.knot.clone()))?;
    sync.probe().await?;

    let local = load_local_keys(&opts.keys_path, opts.create_local)?;
    if let Some(home) = opts.keys_path.parent() {
        persist_knot_if_missing(&knot_path_in(home), &opts.knot)?;
    }

    let local_client = siws_client(&opts.knot, &local).await?;
    let resolved = local_client
        .ap_resolve(&acct)
        .await
        .map_err(|e| ReedError::Handle(e.to_string()))?;
    let subject = Did::parse(&resolved.did).map_err(|e| ReedError::Handle(e.to_string()))?;
    let notes = count_posts(&local_client, &resolved.did).await;
    let (_, records) = publish_posts_and_follows(
        &local,
        &local_client,
        &["watching the federated square"],
        std::slice::from_ref(&subject),
        Some(opts.knot.as_str()),
    )
    .await?;

    Ok(ApSeedReport {
        local_did: local.did().as_str(),
        actor: resolved.actor,
        acct,
        notes,
        records,
    })
}

/// Loopback mock peer (inbox POST). Used by `just reed-ap-seed http://127.0.0.1:3000 --create-local --mock`.
#[cfg(not(target_arch = "wasm32"))]
pub async fn run_ap_mock(opts: SeedOpts) -> Result<ApSeedReport> {
    let sync = Sync::http(KnotUrl(opts.knot.clone()))?;
    sync.probe().await?;

    let local = load_local_keys(&opts.keys_path, opts.create_local)?;
    if let Some(home) = opts.keys_path.parent() {
        persist_knot_if_missing(&knot_path_in(home), &opts.knot)?;
    }

    let peer = tangled_ap::demo::MockPeer::bind("demo")
        .await
        .map_err(|e| ReedError::Io(e.to_string()))?;
    let inbox = format!("{}/inbox", opts.knot.trim_end_matches('/'));
    for text in AP_DEMO_NOTES {
        let delivered = peer
            .post_create_note(&inbox, text)
            .await
            .map_err(|e| ReedError::Protocol(tangled::Error::Transport(e.to_string())))?;
        if delivered.status != 202 {
            return Err(ReedError::Protocol(tangled::Error::Transport(format!(
                "inbox {} {}",
                delivered.status, delivered.body
            ))));
        }
    }

    let local_client = siws_client(&opts.knot, &local).await?;
    let resolved = local_client
        .ap_resolve(&peer.acct())
        .await
        .map_err(|e| ReedError::Handle(e.to_string()))?;
    let subject = Did::parse(&resolved.did).map_err(|e| ReedError::Handle(e.to_string()))?;
    let (_, records) = publish_posts_and_follows(
        &local,
        &local_client,
        &["watching the federated square"],
        std::slice::from_ref(&subject),
        Some(opts.knot.as_str()),
    )
    .await?;

    Ok(ApSeedReport {
        local_did: local.did().as_str(),
        actor: peer.actor_id().to_string(),
        acct: peer.acct(),
        notes: AP_DEMO_NOTES.len(),
        records,
    })
}

async fn count_posts(client: &tangled::http::KnotClient, did: &str) -> usize {
    let Ok(bytes) = client.get_repo(did, None, None).await else {
        return 0;
    };
    let Ok(car) = tangled::car::decode(&bytes) else {
        return 0;
    };
    car.blocks
        .iter()
        .filter(|(_, data)| {
            tangled::dagcbor::decode_json(data)
                .ok()
                .and_then(|v| {
                    v.get("collection")
                        .and_then(|c| c.as_str())
                        .map(|c| c == "pub.tangled.post")
                })
                .unwrap_or(false)
        })
        .count()
}
