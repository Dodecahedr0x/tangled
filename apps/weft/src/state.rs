use dioxus::prelude::WritableExt;
use weft_core::{Explorer, KnotUrl, LookupHit, NetworkView};

#[cfg(not(target_arch = "wasm32"))]
use crate::paths::knot_path;

pub const DEFAULT_KNOT_URL: &str = "http://127.0.0.1:3000";

/// Knot used when none is saved. The hosted build sets `TANGLED_KNOT`.
pub fn default_knot_url() -> &'static str {
    match option_env!("TANGLED_KNOT") {
        Some(url) if !url.is_empty() => url,
        _ => DEFAULT_KNOT_URL,
    }
}

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

pub fn ensure_knot_url() -> String {
    let saved = read_knot();
    if !saved.is_empty() {
        return saved;
    }
    let url = default_knot_url();
    persist_knot(url);
    url.to_string()
}

pub fn read_knot() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("weft.knot").ok().flatten())
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(knot_path())
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

pub fn persist_knot(url: &str) {
    let url = url.trim();
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(store) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let _ = store.set_item("weft.knot", url);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = knot_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, url);
    }
}

pub async fn probe_knot(url: &str) -> KnotStatus {
    let url = url.trim();
    if url.is_empty() {
        return KnotStatus::Unconfigured;
    }
    let Ok(ex) = Explorer::http(KnotUrl(url.to_string())) else {
        return KnotStatus::Unconfigured;
    };
    match ex.local_stats().await {
        Ok(_) => KnotStatus::Ok,
        Err(_) => KnotStatus::Unreachable,
    }
}

#[derive(Clone, PartialEq)]
pub struct Ui {
    pub knot: String,
    pub knot_status: KnotStatus,
    pub view: Option<NetworkView>,
    pub lookup_q: String,
    pub lookup: Option<LookupHit>,
    pub lookup_err: Option<String>,
}

impl Ui {
    pub fn empty() -> Self {
        Self {
            knot: String::new(),
            knot_status: KnotStatus::Unconfigured,
            view: None,
            lookup_q: String::new(),
            lookup: None,
            lookup_err: None,
        }
    }

    pub fn load() -> Self {
        let knot = ensure_knot_url();
        Self {
            knot: knot.clone(),
            knot_status: classify_knot(&knot, None),
            view: None,
            lookup_q: String::new(),
            lookup: None,
            lookup_err: None,
        }
    }
}

pub async fn sync_once(mut ui: dioxus::prelude::Signal<Ui>) {
    let knot = ui().knot.clone();
    let status = probe_knot(&knot).await;
    ui.write().knot_status = status;
    if status != KnotStatus::Ok {
        return;
    }
    let Ok(ex) = Explorer::http(KnotUrl(knot)) else {
        return;
    };
    if let Ok(view) = ex.assemble().await {
        ui.write().view = Some(view);
    }
}

pub async fn run_lookup(mut ui: dioxus::prelude::Signal<Ui>) {
    let (knot, q) = {
        let g = ui();
        (g.knot.clone(), g.lookup_q.clone())
    };
    let Ok(ex) = Explorer::http(KnotUrl(knot)) else {
        ui.write().lookup_err = Some("No knot URL set".into());
        return;
    };
    match ex.lookup(&q).await {
        Ok(hit) => {
            let mut w = ui.write();
            w.lookup = Some(hit);
            w.lookup_err = None;
        }
        Err(_) => {
            let mut w = ui.write();
            w.lookup = None;
            w.lookup_err = Some("Not found".into());
        }
    }
}
