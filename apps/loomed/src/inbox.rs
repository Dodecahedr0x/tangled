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
                    div { class: "brand", "Loomed" }
                    input {
                        r#type: "search",
                        class: "tune",
                        placeholder: "Search",
                        aria_label: "search",
                        value: query,
                        oninput: move |e| on_query.call(e.value()),
                    }
                }
                div { class: "rail-nav",
                    AppLink { to: Route::Inbox {}, class: "rail-link", "Inbox" }
                    AppLink { to: Route::Compose {}, class: "rail-link", "New" }
                    AppLink { to: Route::Settings {}, class: "rail-link", "Settings" }
                    AppLink { to: Route::Onboard {}, class: "rail-link", "Identity" }
                }
            }
            section { class: "stage",
                div { class: "band",
                    if knot_unreachable {
                        span { class: "banner late", {knot_banner()} }
                    }
                }
                div { class: "sheet-cols pair",
                    span { "Who" }
                    span { "Last" }
                }
                if conversations.is_empty() && hits.is_empty() && !knot_unreachable {
                    p { class: "muted empty-stage", "No conversations yet" }
                }
                for c in conversations.iter() {
                    AppLink {
                        class: "conv",
                        to: Route::Thread {
                            circle: c.circle.0.clone(),
                        },
                        span { class: "conv-title",
                            if c.unread > 0 {
                                span { class: "late-mark" }
                            }
                            {
                                let peers = row_peers(c);
                                if peers.is_empty() {
                                    c.title.clone()
                                } else {
                                    recipient_label(&peers, me)
                                }
                            }
                        }
                        span { class: "conv-preview", {c.last_preview.clone()} }
                    }
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
