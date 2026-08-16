//! Local OS notifications (desktop). Tests record calls; no daemon.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use reed_core::{mentions, Replica, SocialEvent, SocialKind, TimelineItem};

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static TOASTS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
}

fn seen() -> &'static Mutex<HashSet<String>> {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashSet::new()))
}

fn seeded() -> &'static std::sync::atomic::AtomicBool {
    static SEEDED: OnceLock<std::sync::atomic::AtomicBool> = OnceLock::new();
    SEEDED.get_or_init(|| std::sync::atomic::AtomicBool::new(false))
}

pub(crate) fn macos_notify_bundle_id() -> &'static str {
    "com.apple.Terminal"
}

pub(crate) fn ensure_macos_application() {
    let bundle = macos_notify_bundle_id();
    #[cfg(all(feature = "desktop", not(test), target_os = "macos"))]
    {
        let _ = notify_rust::set_application(bundle);
    }
    #[cfg(not(all(feature = "desktop", not(test), target_os = "macos")))]
    {
        let _ = bundle;
    }
}

pub fn notify_local(title: &str, body: &str) {
    #[cfg(test)]
    {
        TOASTS.with(|t| t.borrow_mut().push((title.to_string(), body.to_string())));
    }
    #[cfg(all(feature = "desktop", not(test)))]
    {
        ensure_macos_application();
        let _ = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show();
    }
    #[cfg(not(any(test, feature = "desktop")))]
    {
        let _ = (title, body);
    }
}

fn social_preview(items: &[TimelineItem], ev: &SocialEvent) -> String {
    ev.target
        .as_ref()
        .and_then(|id| {
            items
                .iter()
                .find(|i| i.post.id == *id)
                .map(|i| i.post.text.clone())
        })
        .unwrap_or_else(|| ev.actor.clone())
}

pub fn inbound_toasts(
    self_did: &str,
    seen_ids: &HashSet<String>,
    items: &[TimelineItem],
    social: &[SocialEvent],
    handle: Option<&str>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut used = HashSet::new();
    for ev in social {
        if ev.actor == self_did || seen_ids.contains(&ev.id) || !used.insert(ev.id.clone()) {
            continue;
        }
        match ev.kind {
            SocialKind::Follow => out.push(("Follow".into(), ev.actor.clone())),
            SocialKind::Like => out.push(("Like".into(), social_preview(items, ev))),
            SocialKind::Boost => out.push(("Boost".into(), social_preview(items, ev))),
        }
    }
    let mine: HashSet<&str> = items
        .iter()
        .filter(|i| i.post.author == self_did)
        .map(|i| i.post.id.0.as_str())
        .collect();
    for item in items {
        if item.post.author == self_did {
            continue;
        }
        if seen_ids.contains(&item.post.id.0) || !used.insert(item.post.id.0.clone()) {
            continue;
        }
        if item
            .post
            .reply_to
            .as_ref()
            .is_some_and(|p| mine.contains(p.0.as_str()))
        {
            out.push(("Reply".into(), item.post.text.clone()));
        }
        if let Some(h) = handle {
            if mentions(&item.post.text).iter().any(|m| m == h) {
                out.push(("Mention".into(), item.post.text.clone()));
            }
        }
    }
    out
}

fn mark_seen(items: &[TimelineItem], social: &[SocialEvent]) {
    let mut seen_ids = seen().lock().expect("seen");
    for i in items {
        seen_ids.insert(i.post.id.0.clone());
    }
    for ev in social {
        seen_ids.insert(ev.id.clone());
    }
}

pub fn after_replica_snapshot(self_did: &str, replica: &Replica) {
    let mut items = replica.home(self_did);
    items.extend(replica.discover(self_did));
    let social = replica.inbound_social(self_did);
    let handle = replica.profile(self_did).and_then(|p| p.handle);
    if !seeded().swap(true, std::sync::atomic::Ordering::SeqCst) {
        mark_seen(&items, &social);
        return;
    }
    let seen_ids = seen().lock().expect("seen").clone();
    let toasts = inbound_toasts(self_did, &seen_ids, &items, &social, handle.as_deref());
    mark_seen(&items, &social);
    for (title, body) in toasts {
        notify_local(&title, &body);
    }
}

#[cfg(test)]
pub fn take_toasts() -> Vec<(String, String)> {
    TOASTS.with(|t| t.replace(Vec::new()))
}

#[cfg(test)]
pub fn reset_observer() {
    seen().lock().expect("seen").clear();
    seeded().store(false, std::sync::atomic::Ordering::SeqCst);
    TOASTS.with(|t| t.borrow_mut().clear());
}
