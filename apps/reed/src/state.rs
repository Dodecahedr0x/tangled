use dioxus::prelude::WritableExt;
use reed_core::{
    boost, follow, like, parse_peer, resolve_peer, search, unboost, unfollow, unlike, FileKeyStore,
    KeyStore, KnotUrl, Outbox, Peer, Post, PostId, Profile, ReedError, ReedKeys, Replica, Sync,
    TimelineItem,
};

use crate::paths::{keys_path, knot_path, replica_dir};

pub const DEFAULT_KNOT_URL: &str = "http://127.0.0.1:3000";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnotStatus {
    Unconfigured,
    Unreachable,
    Ok,
}

pub fn classify_knot(url: &str, describe_ok: Option<bool>) -> KnotStatus {
    if url.trim().is_empty() {
        return KnotStatus::Unconfigured;
    }
    match describe_ok {
        Some(false) => KnotStatus::Unreachable,
        Some(true) | None => KnotStatus::Ok,
    }
}

pub fn banner_text(status: KnotStatus) -> Option<&'static str> {
    match status {
        KnotStatus::Unconfigured => Some("No knot URL set"),
        KnotStatus::Unreachable => Some("Cannot reach knot"),
        KnotStatus::Ok => None,
    }
}

pub async fn probe_knot(url: &str) -> KnotStatus {
    let url = url.trim();
    if url.is_empty() {
        return KnotStatus::Unconfigured;
    }
    let Ok(sync) = Sync::http(KnotUrl(url.to_string())) else {
        return KnotStatus::Unconfigured;
    };
    match sync.probe().await {
        Ok(()) => KnotStatus::Ok,
        Err(_) => KnotStatus::Unreachable,
    }
}

#[derive(Clone, PartialEq)]
pub struct Ui {
    pub did: Option<String>,
    pub knot: String,
    pub home: Vec<TimelineItem>,
    pub discover: Vec<TimelineItem>,
    pub knot_status: KnotStatus,
    pub query: String,
    pub draft: String,
    pub import_json: String,
}

impl Ui {
    pub fn empty() -> Self {
        Self {
            did: None,
            knot: String::new(),
            home: Vec::new(),
            discover: Vec::new(),
            knot_status: KnotStatus::Unconfigured,
            query: String::new(),
            draft: String::new(),
            import_json: String::new(),
        }
    }

    pub fn load() -> Self {
        let keys = load_keys();
        let did = keys.as_ref().map(|k| k.did().as_str());
        let knot = ensure_knot_url();
        let replica = Replica::open(&replica_dir()).ok();
        let me = did.clone().unwrap_or_default();
        let home = replica.as_ref().map(|r| r.home(&me)).unwrap_or_default();
        let discover = replica
            .as_ref()
            .map(|r| r.discover(&me))
            .unwrap_or_default();
        if let (Some(did), Some(replica)) = (did.as_ref(), replica.as_ref()) {
            crate::notify::after_replica_snapshot(did, replica);
        }
        Self {
            did,
            knot: knot.clone(),
            home,
            discover,
            knot_status: classify_knot(&knot, None),
            query: String::new(),
            draft: String::new(),
            import_json: String::new(),
        }
    }

    pub fn hits(&self) -> Vec<Post> {
        if self.query.is_empty() {
            return Vec::new();
        }
        Replica::open(&replica_dir())
            .map(|r| search(&r, &self.query))
            .unwrap_or_default()
    }
}

pub fn load_keys() -> Option<ReedKeys> {
    FileKeyStore { path: keys_path() }.load().ok().flatten()
}

pub fn persist_keys(keys: &ReedKeys) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = FileKeyStore { path: keys_path() }.save(keys);
    }
    let _ = keys;
}

pub fn persist_knot(url: &str) {
    let _ = std::fs::create_dir_all(crate::paths::home_dir());
    let _ = std::fs::write(knot_path(), url.trim());
}

pub fn read_knot() -> String {
    std::fs::read_to_string(knot_path())
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn ensure_knot_url() -> String {
    let knot = read_knot();
    if knot.is_empty() {
        persist_knot(DEFAULT_KNOT_URL);
        DEFAULT_KNOT_URL.to_string()
    } else {
        knot
    }
}

pub fn apply_onboard_keys(ui: &mut Ui, keys: &ReedKeys) {
    persist_keys(keys);
    ui.did = Some(keys.did().as_str());
    ui.knot = ensure_knot_url();
    ui.knot_status = classify_knot(&ui.knot, None);
}

pub fn profile_for(did: &str) -> Option<Profile> {
    Replica::open(&replica_dir())
        .ok()
        .and_then(|r| r.profile(did))
}

pub fn post_for(id: &PostId) -> Option<Post> {
    Replica::open(&replica_dir()).ok().and_then(|r| r.post(id))
}

pub fn is_following(me: &str, subject: &str) -> bool {
    Replica::open(&replica_dir())
        .map(|r| r.follows(me, subject))
        .unwrap_or(false)
}

pub fn reply_count(id: &str) -> u32 {
    Replica::open(&replica_dir())
        .map(|r| r.reply_count(&PostId(id.to_string())))
        .unwrap_or(0)
}

pub fn refresh_signal(mut ui: dioxus::prelude::Signal<Ui>) {
    let snap = ui();
    let mut next = Ui::load();
    next.query = snap.query.clone();
    next.draft = snap.draft.clone();
    next.import_json = snap.import_json.clone();
    next.knot_status = snap.knot_status;
    ui.set(next);
}

pub fn toggle_like(id: &PostId) -> reed_core::Result<()> {
    let keys = load_keys().ok_or_else(|| ReedError::Identity("no keys".into()))?;
    let mut replica = Replica::open(&replica_dir())?;
    let mut outbox = Outbox::new();
    let me = keys.did().as_str();
    if replica.liked_by(&me, id) {
        unlike(&keys, &mut replica, &mut outbox, id)?;
    } else {
        like(&keys, &mut replica, &mut outbox, id)?;
    }
    drain_outbox(&mut outbox);
    Ok(())
}

pub fn toggle_boost(id: &PostId) -> reed_core::Result<()> {
    let keys = load_keys().ok_or_else(|| ReedError::Identity("no keys".into()))?;
    let mut replica = Replica::open(&replica_dir())?;
    let mut outbox = Outbox::new();
    let me = keys.did().as_str();
    if replica.boosted_by(&me, id) {
        unboost(&keys, &mut replica, &mut outbox, id)?;
    } else {
        boost(&keys, &mut replica, &mut outbox, id)?;
    }
    drain_outbox(&mut outbox);
    Ok(())
}

pub fn toggle_follow(subject: &str) -> reed_core::Result<()> {
    let keys = load_keys().ok_or_else(|| ReedError::Identity("no keys".into()))?;
    let Peer::Did(did) = parse_peer(subject)? else {
        return Err(ReedError::Handle("not a did".into()));
    };
    follow_did(&keys, &did)
}

/// Resolve `acct:user@instance` / actor URL via the knot, then follow.
pub async fn follow_resource(resource: &str) -> reed_core::Result<String> {
    let keys = load_keys().ok_or_else(|| ReedError::Identity("no keys".into()))?;
    let peer = parse_peer(resource)?;
    let did = match &peer {
        Peer::Did(d) => d.clone(),
        Peer::Handle(_) => {
            let sync = Sync::http(KnotUrl(read_knot()))?;
            resolve_peer(&sync.client, &peer).await?
        }
    };
    follow_did(&keys, &did)?;
    Ok(did.as_str())
}

fn follow_did(keys: &ReedKeys, did: &reed_core::Did) -> reed_core::Result<()> {
    let mut replica = Replica::open(&replica_dir())?;
    let mut outbox = Outbox::new();
    let me = keys.did().as_str();
    if replica.follows(&me, &did.as_str()) {
        unfollow(keys, &mut replica, &mut outbox, did)?;
    } else {
        follow(keys, &mut replica, &mut outbox, did, None)?;
    }
    drain_outbox(&mut outbox);
    Ok(())
}

pub fn profile_posts(did: &str) -> Vec<Post> {
    let viewer = load_keys().map(|k| k.did().as_str());
    Replica::open(&replica_dir())
        .map(|r| r.profile_posts_view(did, viewer.as_deref()))
        .unwrap_or_default()
}

pub fn thread_posts(cid: &str) -> Vec<Post> {
    let viewer = load_keys().map(|k| k.did().as_str());
    Replica::open(&replica_dir())
        .map(|r| r.thread_view(&reed_core::PostId(cid.to_string()), viewer.as_deref()))
        .unwrap_or_default()
}

pub fn drain_outbox(outbox: &mut Outbox) {
    if outbox.is_empty() {
        return;
    }
    let knot = read_knot();
    if knot.is_empty() {
        return;
    }
    let Ok(sync) = Sync::http(KnotUrl(knot)) else {
        return;
    };
    let mut pending = std::mem::take(outbox);
    dioxus::prelude::spawn(async move {
        match pending.drain(&sync.client).await {
            Ok(_) | Err(ReedError::KnotUnreachable) => {}
            Err(_) => {}
        }
    });
}

pub async fn poll_and_observe() {
    let Some(keys) = load_keys() else {
        return;
    };
    let did = keys.did().as_str();
    if let Ok(replica) = Replica::open(&replica_dir()) {
        crate::notify::after_replica_snapshot(&did, &replica);
    }
    let knot = read_knot();
    if knot.is_empty() {
        return;
    }
    let Ok(mut sync) = Sync::http(KnotUrl(knot)) else {
        return;
    };
    let Ok(mut replica) = Replica::open(&replica_dir()) else {
        return;
    };
    let _ = sync.poll_once(&mut replica, &keys).await;
    crate::notify::after_replica_snapshot(&did, &replica);
}

pub async fn sync_once(mut ui: dioxus::prelude::Signal<Ui>) {
    poll_and_observe().await;
    let knot = {
        let snap = ui();
        if snap.knot.is_empty() {
            read_knot()
        } else {
            snap.knot.clone()
        }
    };
    let status = probe_knot(&knot).await;
    let snap = ui();
    let mut next = Ui::load();
    next.query = snap.query.clone();
    next.draft = snap.draft.clone();
    next.import_json = snap.import_json.clone();
    next.knot_status = status;
    ui.set(next);
}
