use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};
use crate::{AppLink, Route};

#[component]
pub fn BandRail(
    #[props(default)] show_refresh: bool,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    let context = try_use_context::<Signal<crate::state::Ui>>();
    let host = context.map(|s| s().knot).unwrap_or_default();
    rsx! {
        nav { class: "rail", aria_label: "Application navigation",
            div { class: "rail-head",
                div { class: "brand",
                    span { class: "tick" }
                    "Weft"
                }
                p { class: "rail-description", "A view of the public network" }
            }
            div { class: "rail-nav",
                AppLink { to: Route::Overview {}, class: "rail-link",
                    Icon { glyph: Glyph::Overview }
                    span { "Overview" }
                }
                AppLink { to: Route::Topology {}, class: "rail-link",
                    Icon { glyph: Glyph::Topology }
                    span { "Topology" }
                }
                AppLink { to: Route::Lookup {}, class: "rail-link",
                    Icon { glyph: Glyph::Search }
                    span { "Lookup" }
                }
                AppLink { to: Route::Settings {}, class: "rail-link",
                    Icon { glyph: Glyph::Settings }
                    span { "Settings" }
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
                strong { "Read-only explorer" }
                span { "Configured knot and advertised public peers" }
                if !host.is_empty() { code { "{host}" } }
            }
        }
    }
}
