use dioxus::prelude::*;
use loomed_core::Conversation;

use crate::icons::{Glyph, Icon};
use crate::labels::{recipient_label, PeerLabel};
use crate::{AppLink, Route};

#[component]
pub fn BandRail(
    #[props(default)] conversations: Vec<Conversation>,
    #[props(default)] query: String,
    #[props(default)] show_tune: bool,
    #[props(default)] show_refresh: bool,
    #[props(default)] show_band: bool,
    #[props(default)] active_circle: String,
    #[props(default)] self_did: String,
    #[props(default)] on_query: EventHandler<String>,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    let me = (!self_did.is_empty()).then_some(self_did.as_str());
    rsx! {
        nav { class: "rail",
            div { class: "rail-head",
                div { class: "brand",
                    span { class: "tick" }
                    "loomed"
                }
                if show_tune {
                    label { class: "tune-wrap",
                        Icon { glyph: Glyph::Search }
                        input {
                            r#type: "search",
                            class: "tune",
                            placeholder: "Search",
                            aria_label: "Search conversations",
                            value: query,
                            oninput: move |e| on_query.call(e.value()),
                        }
                    }
                }
            }
            div { class: "rail-list",
                for c in conversations.iter() {
                    AppLink {
                        class: if c.circle.0 == active_circle {
                            "conv is-active"
                        } else {
                            "conv"
                        },
                        to: Route::Thread {
                            circle: c.circle.0.clone(),
                        },
                        span { class: "tick" }
                        span { class: "conv-meta",
                            span { class: "conv-title", {
                                let peers: Vec<PeerLabel> = c
                                    .members
                                    .iter()
                                    .map(|did| PeerLabel::new(did.clone(), None))
                                    .collect();
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
                if show_band {
                    AppLink { to: Route::Inbox {}, class: "rail-link",
                        Icon { glyph: Glyph::Chats }
                        span { "Chats" }
                    }
                }
                AppLink { to: Route::Compose {}, class: "rail-link",
                    Icon { glyph: Glyph::NewChat }
                    span { "New chat" }
                }
                AppLink { to: Route::Settings {}, class: "rail-link",
                    Icon { glyph: Glyph::Settings }
                    span { "Settings" }
                }
                AppLink { to: Route::Onboard {}, class: "rail-link",
                    Icon { glyph: Glyph::Identity }
                    span { "Identity" }
                }
                if show_refresh {
                    button {
                        class: "ghost rail-link",
                        r#type: "button",
                        title: "Refresh",
                        onclick: move |_| on_refresh.call(()),
                        Icon { glyph: Glyph::Refresh }
                        span { "Refresh" }
                    }
                }
            }
        }
    }
}
