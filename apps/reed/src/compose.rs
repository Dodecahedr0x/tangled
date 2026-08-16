use dioxus::prelude::*;
use reed_core::{PostApi, PostId, Replica};

use crate::paths::replica_dir;
use crate::rail::BandRail;
use crate::state::{drain_outbox, load_keys, Ui};

#[component]
pub fn Compose() -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let self_did = ui
        .map(|s| s().did.clone().unwrap_or_default())
        .unwrap_or_default();
    rsx! { ComposeView { self_did } }
}

#[component]
pub fn ComposeView(#[props(default)] self_did: String) -> Element {
    let mut draft = use_signal(String::new);
    let mut quote = use_signal(String::new);
    let mut err = use_signal(String::new);
    rsx! {
        div { class: "shell",
            BandRail { self_did }
            section { class: "stage",
                div { class: "stage-panel",
                    p { class: "page-title",
                        span { class: "tick" }
                        "Compose"
                    }
                    textarea {
                        placeholder: "What's happening",
                        value: draft,
                        oninput: move |e| draft.set(e.value()),
                    }
                    div { class: "field",
                        label { class: "field-label", r#for: "quote-cid", "Quote CID (optional)" }
                        input {
                            id: "quote-cid",
                            placeholder: "original post CID",
                            value: quote,
                            oninput: move |e| quote.set(e.value()),
                        }
                    }
                    button {
                        onclick: move |_| {
                            match publish(&draft(), quote().as_str()) {
                                Ok(_) => {
                                    draft.set(String::new());
                                    quote.set(String::new());
                                    err.set(String::new());
                                }
                                Err(e) => err.set(e.to_string()),
                            }
                        },
                        "Post"
                    }
                    if !err().is_empty() {
                        p { class: "banner", {err()} }
                    }
                }
            }
        }
    }
}

fn publish(text: &str, quote_cid: &str) -> reed_core::Result<PostId> {
    let keys = load_keys().ok_or_else(|| reed_core::ReedError::Identity("no keys".into()))?;
    let mut replica = Replica::open(&replica_dir())?;
    let mut outbox = reed_core::Outbox::new();
    let api = PostApi { keys };
    let quote = if quote_cid.trim().is_empty() {
        None
    } else {
        Some(PostId(quote_cid.trim().to_string()))
    };
    let id = api.compose_text(&mut replica, &mut outbox, text, None, quote.as_ref())?;
    drain_outbox(&mut outbox);
    Ok(id)
}
