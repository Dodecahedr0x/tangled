use std::time::Duration;

use dioxus::prelude::*;
use loomed_core::{
    search, CircleRef, Conversation, FileKeyStore, KeyStore, KnotUrl, LoomedError, LoomedKeys,
    Message, Replica, Sync,
};

use crate::paths::{keys_path, knot_path, replica_dir};

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, PartialEq)]
pub struct Ui {
    pub did: Option<String>,
    pub knot: String,
    pub conversations: Vec<Conversation>,
    pub knot_unreachable: bool,
    pub query: String,
    pub draft: String,
    pub peer: String,
    pub import_json: String,
}

impl Ui {
    pub fn empty() -> Self {
        Self {
            did: None,
            knot: String::new(),
            conversations: Vec::new(),
            knot_unreachable: false,
            query: String::new(),
            draft: String::new(),
            peer: String::new(),
            import_json: String::new(),
        }
    }

    pub fn load() -> Self {
        let keys = load_keys();
        let did = keys.as_ref().map(|k| k.did().as_str());
        let knot = std::fs::read_to_string(knot_path()).unwrap_or_default();
        let knot = knot.trim().to_string();
        let replica = Replica::open(&replica_dir()).ok();
        let conversations = replica
            .as_ref()
            .map(|r| r.conversations())
            .unwrap_or_default();
        if let (Some(did), Some(replica)) = (did.as_ref(), replica.as_ref()) {
            crate::notify::after_replica_snapshot(did, replica);
        }
        // Local only. A configured URL is checked off the UI thread in bootstrap.
        let knot_unreachable = knot.is_empty();
        Self {
            did,
            knot,
            conversations,
            knot_unreachable,
            query: String::new(),
            draft: String::new(),
            peer: String::new(),
            import_json: String::new(),
        }
    }

    pub fn hits(&self) -> Vec<Message> {
        if self.query.is_empty() {
            return Vec::new();
        }
        Replica::open(&replica_dir())
            .map(|r| search(&r, &self.query))
            .unwrap_or_default()
    }
}

pub fn load_keys() -> Option<LoomedKeys> {
    FileKeyStore { path: keys_path() }.load().ok().flatten()
}

pub fn persist_keys(keys: &LoomedKeys) {
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

pub fn messages_for(circle: &str) -> Vec<Message> {
    Replica::open(&replica_dir())
        .map(|r| r.messages(&CircleRef(circle.to_string())))
        .unwrap_or_default()
}

pub fn knot_banner() -> String {
    LoomedError::KnotUnreachable.to_string()
}

fn read_knot() -> String {
    std::fs::read_to_string(knot_path())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Run `fut` on a worker thread. `dioxus::spawn` / `use_future` poll on the
/// VirtualDom thread; reqwest `describe` / `poll_once` there freezes the window.
async fn run_off_ui<T: Send + 'static>(
    fut: impl std::future::Future<Output = T> + Send + 'static,
) -> Result<T, ()> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return handle.spawn(fut).await.map_err(|_| ());
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("knot runtime");
        let _ = tx.send(rt.block_on(fut));
    });
    rx.await.map_err(|_| ())
}

/// Probe + poll the knot. Returns whether the knot is unreachable.
pub async fn probe_and_poll() -> bool {
    let knot = read_knot();
    if knot.is_empty() {
        return true;
    }
    run_off_ui(async move {
        let Ok(mut sync) = Sync::http(KnotUrl(knot)) else {
            return true;
        };
        let reachable = tokio::time::timeout(PROBE_TIMEOUT, sync.probe())
            .await
            .ok()
            .and_then(Result::ok)
            .is_some();
        if !reachable {
            return true;
        }
        if let (Some(keys), Ok(mut replica)) = (load_keys(), Replica::open(&replica_dir())) {
            let _ = tokio::time::timeout(POLL_TIMEOUT, sync.poll_once(&mut replica, &keys)).await;
        }
        false
    })
    .await
    .unwrap_or(true)
}

/// After first paint: sync off the UI thread, then refresh the shell from disk.
pub async fn bootstrap(mut ui: Signal<Ui>) {
    let snap = ui();
    let query = snap.query.clone();
    let draft = snap.draft.clone();
    let peer = snap.peer.clone();
    let import_json = snap.import_json.clone();
    let unreachable = probe_and_poll().await;
    let mut next = Ui::load();
    next.query = query;
    next.draft = draft;
    next.peer = peer;
    next.import_json = import_json;
    next.knot_unreachable = unreachable;
    ui.set(next);
}

/// Poll the knot then toast newly observed inbound messages. No-op without keys/knot.
#[cfg(feature = "desktop")]
pub async fn poll_and_observe() {
    let _ = probe_and_poll().await;
    let Some(keys) = load_keys() else {
        return;
    };
    let did = keys.did().as_str();
    if let Ok(replica) = Replica::open(&replica_dir()) {
        crate::notify::after_replica_snapshot(&did, &replica);
    }
}
