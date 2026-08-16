use dioxus::prelude::*;
use reed_core::TimelineItem;

use crate::post_row::TimelineCard;
use crate::rail::BandRail;
use crate::state::{banner_text, KnotStatus, Ui};

#[component]
pub fn Home() -> Element {
    let mut ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    rsx! {
        HomeView {
            items: ui().home.clone(),
            knot_unreachable: ui().knot_status == KnotStatus::Unreachable,
            knot_unconfigured: ui().knot_status == KnotStatus::Unconfigured,
            query: ui().query.clone(),
            self_did: ui().did.clone().unwrap_or_default(),
            on_query: move |q| { ui.write().query = q; },
            on_refresh: move |_| {
                spawn(async move { crate::state::sync_once(ui).await; });
            },
        }
    }
}

#[component]
pub fn HomeView(
    items: Vec<TimelineItem>,
    knot_unreachable: bool,
    #[props(default)] knot_unconfigured: bool,
    #[props(default)] query: String,
    #[props(default)] self_did: String,
    #[props(default)] on_query: EventHandler<String>,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "shell",
            BandRail {
                query: query.clone(),
                show_tune: true,
                show_refresh: true,
                self_did: self_did.clone(),
                on_query,
                on_refresh,
            }
            section { class: "stage",
                if knot_unconfigured {
                    div { class: "banner", {banner_text(KnotStatus::Unconfigured).unwrap_or("No knot URL set")} }
                } else if knot_unreachable {
                    div { class: "banner", {banner_text(KnotStatus::Unreachable).unwrap_or("Cannot reach knot")} }
                }
                p { class: "page-title",
                    span { class: "tick" }
                    "Home"
                }
                if items.is_empty() {
                    div { class: "empty",
                        span { class: "tick" }
                        p { "No posts yet" }
                        p { class: "muted", "Follow someone or compose. History stays local." }
                    }
                } else {
                    div { class: "log",
                        for item in items.iter() {
                            TimelineCard { item: item.clone() }
                        }
                    }
                }
            }
        }
    }
}
