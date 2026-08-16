use dioxus::prelude::*;
use loomed_core::{parse_peer, Chat, Outbox, Peer, Replica};

use crate::paths::replica_dir;
use crate::state::load_keys;
use crate::{AppLink, Route};

#[component]
pub fn Compose() -> Element {
    let mut peer = use_signal(String::new);
    let mut note = use_signal(String::new);
    rsx! {
        div { class: "shell",
            nav { class: "rail",
                div { class: "rail-head",
                    AppLink { to: Route::Inbox {}, class: "muted rail-link", "band" }
                }
            }
            section { class: "stage stage-pad",
                p { class: "brand",
                    span { class: "tick" }
                    "new"
                }
                input {
                    r#type: "text",
                    placeholder: "did:sol or handle",
                    value: peer,
                    oninput: move |e| peer.set(e.value()),
                }
                button {
                    onclick: move |_| {
                        note.set(open_circle(&peer()));
                    },
                    "open"
                }
                if !note().is_empty() {
                    p { class: "muted", {note()} }
                }
            }
        }
    }
}

fn open_circle(raw: &str) -> String {
    let Some(keys) = load_keys() else {
        return "generate identity first".into();
    };
    match parse_peer(raw) {
        Ok(Peer::Did(did)) => {
            let chat = Chat { keys };
            match chat.create_circle(&[did]) {
                Ok((circle, rec)) => {
                    let mut box_ = Outbox::new();
                    let _ = box_.enqueue_record(rec.clone());
                    if let Ok(mut replica) = Replica::open(&replica_dir()) {
                        let _ = replica.apply_record(rec.to_string().as_bytes());
                    }
                    circle.0
                }
                Err(e) => e.to_string(),
            }
        }
        Ok(Peer::Handle(h)) => format!("handle {h}: resolve on the knot"),
        Err(e) => e.to_string(),
    }
}
