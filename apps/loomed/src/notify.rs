//! Local OS notifications (desktop). Tests record calls; no daemon.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use loomed_core::{Conversation, Message, Replica};

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

fn prev_unread() -> &'static Mutex<HashMap<String, u32>> {
    static PREV: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    PREV.get_or_init(|| Mutex::new(HashMap::new()))
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

pub fn inbound_toasts(
    self_did: &str,
    seen_ids: &HashSet<String>,
    prev_unread: &HashMap<String, u32>,
    conversations: &[Conversation],
    messages: &[Message],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut toasted = HashSet::new();
    for m in messages {
        if seen_ids.contains(&m.id.0) || m.author == self_did {
            continue;
        }
        let conv = conversations.iter().find(|c| c.circle == m.circle);
        let title = conv
            .map(|c| c.title.as_str())
            .filter(|t| !t.is_empty())
            .unwrap_or("chat");
        let body = m.text.clone().unwrap_or_else(|| {
            if m.blob_cid.is_some() {
                "[image]".into()
            } else {
                conv.map(|c| c.last_preview.clone()).unwrap_or_default()
            }
        });
        if body.is_empty() {
            continue;
        }
        out.push((title.to_string(), body));
        toasted.insert(m.circle.0.clone());
    }
    for c in conversations {
        let prev = prev_unread.get(&c.circle.0).copied().unwrap_or(0);
        if c.unread <= prev || toasted.contains(&c.circle.0) || c.last_preview.is_empty() {
            continue;
        }
        let new_here: Vec<_> = messages
            .iter()
            .filter(|m| m.circle == c.circle && !seen_ids.contains(&m.id.0))
            .collect();
        if !new_here.is_empty() && new_here.iter().all(|m| m.author == self_did) {
            continue;
        }
        out.push((c.title.clone(), c.last_preview.clone()));
    }
    out
}

pub fn observe_inbound(self_did: &str, conversations: &[Conversation], messages: &[Message]) {
    let mut seen_ids = seen().lock().expect("seen");
    let mut unread = prev_unread().lock().expect("unread");
    let toasts = inbound_toasts(self_did, &seen_ids, &unread, conversations, messages);
    for m in messages {
        seen_ids.insert(m.id.0.clone());
    }
    for c in conversations {
        unread.insert(c.circle.0.clone(), c.unread);
    }
    drop(seen_ids);
    drop(unread);
    for (title, body) in toasts {
        notify_local(&title, &body);
    }
}

pub fn after_replica_snapshot(self_did: &str, replica: &Replica) {
    let conversations = replica.conversations();
    let mut messages = Vec::new();
    for c in &conversations {
        messages.extend(replica.messages(&c.circle));
    }
    if !seeded().swap(true, std::sync::atomic::Ordering::SeqCst) {
        let mut seen_ids = seen().lock().expect("seen");
        for m in &messages {
            seen_ids.insert(m.id.0.clone());
        }
        let mut unread = prev_unread().lock().expect("unread");
        for c in &conversations {
            unread.insert(c.circle.0.clone(), c.unread);
        }
        return;
    }
    observe_inbound(self_did, &conversations, &messages);
}

#[cfg(test)]
pub fn take_toasts() -> Vec<(String, String)> {
    TOASTS.with(|t| t.replace(Vec::new()))
}

#[cfg(test)]
pub fn reset_observer() {
    seen().lock().expect("seen").clear();
    prev_unread().lock().expect("unread").clear();
    seeded().store(false, std::sync::atomic::Ordering::SeqCst);
    TOASTS.with(|t| t.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use loomed_core::{CircleRef, Conversation, Message, MessageId};

    use super::*;

    fn conv() -> Conversation {
        Conversation {
            circle: CircleRef("did:sol:alice#r1".into()),
            title: "bob".into(),
            last_preview: "ping".into(),
            unread: 1,
            members: vec!["did:sol:alice".into(), "did:sol:bob".into()],
        }
    }

    fn msg(id: &str, author: &str, text: &str) -> Message {
        Message {
            id: MessageId(id.into()),
            circle: CircleRef("did:sol:alice#r1".into()),
            author: author.into(),
            text: Some(text.into()),
            blob_cid: None,
            ts_us: 1,
            receipts: vec![],
        }
    }

    #[test]
    fn notify_helper_inbound_not_own_send() {
        reset_observer();
        let c = conv();
        observe_inbound(
            "did:sol:alice",
            std::slice::from_ref(&c),
            &[msg("m1", "did:sol:bob", "ping")],
        );
        assert_eq!(take_toasts(), vec![("bob".into(), "ping".into())]);

        reset_observer();
        observe_inbound(
            "did:sol:alice",
            &[c],
            &[msg("m2", "did:sol:alice", "hello band")],
        );
        assert!(take_toasts().is_empty(), "own send must not fire OS notify");
    }
}
