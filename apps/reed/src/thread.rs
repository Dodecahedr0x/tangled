use dioxus::prelude::*;
use reed_core::{Post, PostApi, PostId, Replica};

use crate::paths::replica_dir;
use crate::post_row::{PostRow, RowKind};
use crate::rail::BandRail;
use crate::state::{drain_outbox, load_keys, Ui};

#[component]
pub fn Thread(cid: String) -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let posts = crate::state::thread_posts(&cid);
    let self_did = ui
        .map(|s| s().did.clone().unwrap_or_default())
        .unwrap_or_default();
    let draft = ui.map(|s| s().draft.clone()).unwrap_or_default();
    rsx! {
        ThreadView { cid, posts, draft, self_did }
    }
}

#[component]
pub fn ThreadView(cid: String, posts: Vec<Post>, draft: String, self_did: String) -> Element {
    let mut draft = use_signal(|| draft);
    let root = posts
        .first()
        .map(|p| p.id.0.clone())
        .unwrap_or_else(|| cid.clone());
    rsx! {
        div { class: "shell",
            BandRail { self_did: self_did.clone() }
            section { class: "stage",
                p { class: "page-title",
                    span { class: "tick" }
                    "Thread"
                }
                div { class: "log thread-log",
                    for (i, p) in posts.iter().enumerate() {
                        PostRow {
                            post: p.clone(),
                            kind: if i == 0 || p.id.0 == root { RowKind::Parent } else { RowKind::Reply },
                        }
                    }
                }
                div { class: "compose",
                    input {
                        value: draft,
                        placeholder: "Reply",
                        oninput: move |e| draft.set(e.value()),
                    }
                    button {
                        onclick: move |_| {
                            let text = draft();
                            if text.is_empty() { return; }
                            if transmit(&cid, &text).is_ok() {
                                draft.set(String::new());
                            }
                        },
                        "Send"
                    }
                }
            }
        }
    }
}

pub fn transmit(parent: &str, text: &str) -> reed_core::Result<PostId> {
    let keys = load_keys().ok_or_else(|| reed_core::ReedError::Identity("no keys".into()))?;
    let mut replica = Replica::open(&replica_dir())?;
    let mut outbox = reed_core::Outbox::new();
    let api = PostApi { keys };
    let id = api.compose_text(
        &mut replica,
        &mut outbox,
        text,
        Some(&PostId(parent.to_string())),
        None,
    )?;
    drain_outbox(&mut outbox);
    Ok(id)
}
