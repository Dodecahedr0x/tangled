use dioxus::prelude::*;
use reed_core::TimelineItem;

use crate::post_row::TimelineCard;
use crate::rail::BandRail;
use crate::state::{banner_text, KnotStatus, Ui};

#[component]
pub fn Discover() -> Element {
    let ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    rsx! {
        DiscoverView {
            items: ui().discover.clone(),
            knot_unreachable: ui().knot_status == KnotStatus::Unreachable,
            self_did: ui().did.clone().unwrap_or_default(),
            on_refresh: move |_| {
                spawn(async move { crate::state::sync_once(ui).await; });
            },
        }
    }
}

#[component]
pub fn DiscoverView(
    items: Vec<TimelineItem>,
    knot_unreachable: bool,
    #[props(default)] self_did: String,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "shell",
            BandRail { show_refresh: true, self_did, on_refresh }
            main { class: "stage", id: "main-content",
                crate::suite::SuiteBar { current: "Reed" }
                if knot_unreachable {
                    div { class: "banner", {banner_text(KnotStatus::Unreachable).unwrap_or("Cannot reach knot")} }
                }
                h1 { class: "page-title",
                    span { class: "tick" }
                    "Discover"
                }
                if items.is_empty() {
                    div { class: "empty",
                        p { "No public posts" }
                        p { class: "muted", "Pull a knot to fill Discover." }
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
