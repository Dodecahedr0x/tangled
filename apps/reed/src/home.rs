use crate::{AppLink, Route};
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
            main { class: "stage", id: "main-content",
                crate::suite::SuiteBar { current: "Reed" }
                div { class: "band",
                    if knot_unconfigured {
                        span { class: "banner", {banner_text(KnotStatus::Unconfigured).unwrap_or("No knot URL set")} }
                    } else if knot_unreachable {
                        span { class: "banner late", {banner_text(KnotStatus::Unreachable).unwrap_or("Cannot reach knot")} }
                    }
                }
                div { class: "page-heading",
                    h1 { "Home" }
                    span { class: "scope-label", "Public conversation" }
                }
                div { class: "home-workspace",
                    div { class: "home-feed",
                        nav { class: "feed-tabs", aria_label: "Feed",
                            AppLink { to: Route::Home {}, class: "current", "Following" }
                            AppLink { to: Route::Discover {}, "Discover" }
                        }
                        div { class: "home-composer",
                            p { "What would you like to share?" }
                            span { "Posts on Reed are public" }
                            AppLink { to: Route::Compose {}, class: "button", "Compose a post" }
                        }
                if items.is_empty() {
                    div { class: "empty",
                        p { "No posts yet" }
                        p { class: "muted", "Follow someone or compose. History stays local." }
                    }
                } else {
                    div { class: "log sheet",
                        for item in items.iter() {
                            TimelineCard { item: item.clone() }
                        }
                    }
                }
                    }
                    aside { class: "home-context",
                        section { class: "context-panel",
                            h2 { "Your identity" }
                            p { "Keys on this device" }
                            if !self_did.is_empty() { p { class: "did", "{self_did}" } }
                            p { "Reed signs your public posts using your local identity." }
                            AppLink { to: Route::Onboard {}, "Manage identity" }
                        }
                        section { class: "context-panel",
                            h2 { "A shared place, your own keys" }
                            p { "Reed posts and replies are public. Private conversations belong in Loomed." }
                            p { "Your provider account is separate. Signing in there does not sign you in to Reed." }
                            AppLink { to: Route::Settings {}, "Host settings" }
                        }
                    }
                }
            }
        }
    }
}
