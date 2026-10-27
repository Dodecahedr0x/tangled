use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};
use crate::{AppLink, Route};

#[component]
pub fn BandRail(
    #[props(default)] query: String,
    #[props(default)] show_tune: bool,
    #[props(default)] show_refresh: bool,
    #[props(default)] on_query: EventHandler<String>,
    #[props(default)] on_refresh: EventHandler<()>,
    #[props(default)] self_did: String,
) -> Element {
    let context = try_use_context::<Signal<crate::state::Ui>>();
    let host = context.map(|s| s().knot).unwrap_or_default();
    let self_did = if self_did.is_empty() {
        context.and_then(|s| s().did).unwrap_or_default()
    } else {
        self_did
    };
    rsx! {
        nav { class: "rail", aria_label: "Application navigation",
            div { class: "rail-head",
                div { class: "brand",
                    span { class: "tick" }
                    "Reed"
                }
                p { class: "rail-description", "A place for public conversation" }
                if show_tune {
                    label { class: "tune-wrap",
                        Icon { glyph: Glyph::Search }
                        input {
                            r#type: "search",
                            class: "tune",
                            placeholder: "Search",
                            aria_label: "Search posts",
                            value: query,
                            oninput: move |e| on_query.call(e.value()),
                        }
                    }
                }
            }
            div { class: "rail-nav",
                AppLink { to: Route::Home {}, class: "rail-link",
                    Icon { glyph: Glyph::Home }
                    span { "Home" }
                }
                AppLink { to: Route::Discover {}, class: "rail-link",
                    Icon { glyph: Glyph::Discover }
                    span { "Discover" }
                }
                AppLink { to: Route::Compose {}, class: "rail-link",
                    Icon { glyph: Glyph::Compose }
                    span { "Compose" }
                }
                AppLink { to: Route::Search {}, class: "rail-link",
                    Icon { glyph: Glyph::Search }
                    span { "Search" }
                }
                if !self_did.is_empty() {
                    AppLink { to: Route::Profile { did: self_did.clone() }, class: "rail-link",
                        Icon { glyph: Glyph::Identity }
                        span { "Profile" }
                    }
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
                        onclick: move |_| on_refresh.call(()),
                        Icon { glyph: Glyph::Refresh }
                        span { "Refresh" }
                    }
                }
            }
            div { class: "identity-anchor",
                strong { "Keys on this device" }
                if !self_did.is_empty() { code { "{self_did}" } } else { AppLink { to: Route::Onboard {}, "Set up your identity" } }
                if !host.is_empty() { code { "{host}" } }
            }
        }
    }
}
