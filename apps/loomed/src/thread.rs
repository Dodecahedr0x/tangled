use dioxus::prelude::*;
use loomed_core::{
    Chat, CircleRef, KnotUrl, LoomedError, Message, MessageId, Outbox, Replica, Result, Sync,
};

use crate::labels::{
    group_subtitle, infer_recipients, member_count, peer_display, recipient_label, PeerLabel,
};
use crate::paths::{knot_path, replica_dir};
use crate::state::{load_keys, messages_for, Ui};
use crate::{AppLink, Route};

const CLUSTER_US: u64 = 2 * 60 * 1_000_000;

fn author_display(author: &str, peers: &[PeerLabel]) -> String {
    let handle = peers
        .iter()
        .find(|p| p.did == author)
        .and_then(|p| p.handle.as_deref());
    peer_display(author, handle)
}

/// Local 24h clock (`14:42`). Not a full ISO datetime.
pub(crate) fn short_time(ts_us: u64) -> String {
    let secs = ts_us / 1_000_000;
    let (hour, min) = local_hour_min(secs);
    format!("{hour}:{min:02}")
}

fn local_hour_min(unix_secs: u64) -> (u32, u32) {
    #[cfg(unix)]
    {
        #[repr(C)]
        struct Tm {
            tm_sec: i32,
            tm_min: i32,
            tm_hour: i32,
            tm_mday: i32,
            tm_mon: i32,
            tm_year: i32,
            tm_wday: i32,
            tm_yday: i32,
            tm_isdst: i32,
            tm_gmtoff: i64,
            tm_zone: *const i8,
        }
        extern "C" {
            fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
        }
        unsafe {
            let t = unix_secs as i64;
            let mut tm = std::mem::zeroed::<Tm>();
            if !localtime_r(&t, &mut tm).is_null() && tm.tm_hour >= 0 && tm.tm_min >= 0 {
                return (tm.tm_hour as u32, tm.tm_min as u32);
            }
        }
    }
    let mins = unix_secs / 60;
    (((mins / 60) % 24) as u32, (mins % 60) as u32)
}

fn clustered_with(a: &Message, b: &Message) -> bool {
    a.author == b.author && a.ts_us.abs_diff(b.ts_us) <= CLUSTER_US
}

pub fn transmit(circle: &str, text: &str) -> Result<MessageId> {
    let keys = load_keys().ok_or_else(|| LoomedError::Identity("no keys".into()))?;
    let mut replica = Replica::open(&replica_dir())?;
    let mut outbox = Outbox::new();
    let chat = Chat { keys };
    let id = chat.send_text(
        &mut replica,
        &mut outbox,
        &CircleRef(circle.to_string()),
        text,
    )?;
    try_drain(&mut outbox);
    Ok(id)
}

fn try_drain(outbox: &mut Outbox) {
    if outbox.is_empty() {
        return;
    }
    let knot = std::fs::read_to_string(knot_path()).unwrap_or_default();
    let knot = knot.trim().to_string();
    if knot.is_empty() {
        return;
    }
    let Ok(sync) = Sync::http(KnotUrl(knot)) else {
        return;
    };
    let mut pending = std::mem::take(outbox);
    spawn(async move {
        match pending.drain(&sync.client).await {
            Ok(_) | Err(LoomedError::KnotUnreachable) => {}
            Err(_) => {}
        }
    });
}

fn circle_recipients(circle: &str) -> Vec<PeerLabel> {
    Replica::open(&replica_dir())
        .ok()
        .and_then(|r| r.circle_state(&CircleRef(circle.to_string())))
        .map(|st| {
            st.members
                .into_iter()
                .map(|did| PeerLabel::new(did, None))
                .collect()
        })
        .unwrap_or_default()
}

#[component]
pub fn Thread(circle: String) -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let draft = ui.as_ref().map(|s| s().draft).unwrap_or_default();
    let self_did = ui.and_then(|s| s().did).unwrap_or_default();
    let messages = messages_for(&circle);
    let recipients = circle_recipients(&circle);
    rsx! {
        ThreadView {
            circle,
            messages,
            draft,
            self_did,
            recipients,
        }
    }
}

#[component]
pub fn ThreadView(
    circle: String,
    messages: Vec<Message>,
    draft: String,
    #[props(default)] self_did: String,
    #[props(default)] recipients: Vec<PeerLabel>,
) -> Element {
    let mut draft = use_signal(|| draft);
    let mut messages = use_signal(|| messages);
    let circle_send = circle.clone();
    let shown = messages();
    let peers = if recipients.is_empty() {
        infer_recipients(&circle, &shown)
    } else {
        recipients
    };
    let me = if self_did.is_empty() {
        None
    } else {
        Some(self_did.as_str())
    };
    let title = recipient_label(&peers, me);
    let subtitle = group_subtitle(member_count(&peers));
    rsx! {
        div { class: "shell",
            nav { class: "rail",
                div { class: "rail-head",
                    AppLink { to: Route::Inbox {}, class: "muted rail-link", "band" }
                }
            }
            section { class: "stage",
                header { class: "thread-header",
                    p { class: "thread-header-title",
                        span { class: "tick" }
                        {title}
                    }
                    if let Some(sub) = subtitle {
                        p { class: "thread-header-sub muted", {sub} }
                    }
                }
                div { class: "log thread-log",
                    for (i, m) in shown.iter().enumerate() {
                        {
                            let own = !self_did.is_empty() && m.author == self_did;
                            let show_name = shown
                                .get(i.wrapping_sub(1))
                                .map(|prev| !clustered_with(prev, m))
                                .unwrap_or(true);
                            let show_time = shown
                                .get(i + 1)
                                .map(|next| !clustered_with(m, next))
                                .unwrap_or(true);
                            let mut class = String::from("msg");
                            if own {
                                class.push_str(" is-own");
                            }
                            if !show_name {
                                class.push_str(" is-cont");
                            }
                            let name = author_display(&m.author, &peers);
                            let time = short_time(m.ts_us);
                            rsx! {
                                article { class,
                                    if show_name {
                                        span {
                                            class: "msg-name muted",
                                            title: m.author.clone(),
                                            {name}
                                        }
                                    }
                                    if m.text.is_some() || !m.receipts.is_empty() {
                                        div { class: "msg-bubble",
                                            if let Some(text) = m.text.clone() {
                                                p { {text} }
                                            }
                                            if !m.receipts.is_empty() {
                                                ul { class: "receipts",
                                                    for r in m.receipts.iter() {
                                                        li { {r.clone()} }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if show_time {
                                        time { class: "msg-time muted", {time} }
                                    }
                                }
                            }
                        }
                    }
                }
                form {
                    class: "compose",
                    onsubmit: move |e| {
                        e.prevent_default();
                        let text = draft();
                        if text.trim().is_empty() {
                            return;
                        }
                        if transmit(&circle_send, &text).is_ok() {
                            messages.set(messages_for(&circle_send));
                            draft.set(String::new());
                        }
                    },
                    input {
                        r#type: "text",
                        name: "compose",
                        placeholder: "transmit",
                        value: draft,
                        oninput: move |e| draft.set(e.value()),
                    }
                    button { r#type: "submit", "send" }
                }
            }
        }
    }
}
