use dioxus::prelude::*;
use loomed_core::{Conversation, Message};

use crate::labels::{recipient_label, PeerLabel};
use crate::state::{knot_banner, Ui};
use crate::{AppLink, Route};

fn row_peers(c: &Conversation) -> Vec<PeerLabel> {
    c.members
        .iter()
        .map(|did| PeerLabel::new(did.clone(), None))
        .collect()
}

#[component]
pub fn Inbox() -> Element {
    let mut ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    let snap = ui();
    rsx! {
        InboxView {
            conversations: snap.conversations.clone(),
            knot_unreachable: snap.knot_unreachable,
            query: snap.query.clone(),
            hits: snap.hits(),
            self_did: snap.did.clone().unwrap_or_default(),
            on_query: move |q| {
                ui.write().query = q;
            },
        }
    }
}

#[component]
pub fn InboxView(
    conversations: Vec<Conversation>,
    knot_unreachable: bool,
    query: String,
    #[props(default)] hits: Vec<Message>,
    #[props(default)] self_did: String,
    #[props(default)] on_query: EventHandler<String>,
) -> Element {
    let me = (!self_did.is_empty()).then_some(self_did.as_str());
    rsx! {
        div { class: "shell",
            nav { class: "rail",
                div { class: "rail-head",
                    div { class: "brand",
                        span { class: "tick" }
                        "loomed"
                    }
                    input {
                        r#type: "search",
                        class: "tune",
                        placeholder: "tune",
                        aria_label: "search",
                        value: query,
                        oninput: move |e| on_query.call(e.value()),
                    }
                }
                div { class: "rail-list",
                    if conversations.is_empty() {
                        p { class: "muted empty-rail", "the band is quiet" }
                    }
                    for c in conversations.iter() {
                        AppLink {
                            class: "conv",
                            to: Route::Thread {
                                circle: c.circle.0.clone(),
                            },
                            span { class: "tick" }
                            span { class: "conv-meta",
                                span { class: "conv-title", {
                                    let peers = row_peers(c);
                                    if peers.is_empty() {
                                        c.title.clone()
                                    } else {
                                        recipient_label(&peers, me)
                                    }
                                } }
                                span { class: "muted conv-preview", {c.last_preview.clone()} }
                            }
                        }
                    }
                }
                div { class: "rail-nav",
                    AppLink { to: Route::Compose {}, class: "muted rail-link", "new" }
                    AppLink { to: Route::Settings {}, class: "muted rail-link", "set" }
                    AppLink { to: Route::Onboard {}, class: "muted rail-link", "id" }
                }
            }
            section { class: "stage",
                if knot_unreachable {
                    div { class: "banner", {knot_banner()} }
                }
                if conversations.is_empty() && hits.is_empty() && !knot_unreachable {
                    p { class: "muted empty-stage", "the band is quiet" }
                }
                for m in hits.iter() {
                    article { class: "tx",
                        span { class: "muted", {m.author.clone()} }
                        if let Some(text) = m.text.clone() {
                            p { {text} }
                        }
                    }
                }
            }
        }
    }
}
