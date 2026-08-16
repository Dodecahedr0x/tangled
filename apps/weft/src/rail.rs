use dioxus::prelude::*;

use crate::icons::{Glyph, Icon};
use crate::{AppLink, Route};

#[component]
pub fn BandRail(
    #[props(default)] show_refresh: bool,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        nav { class: "rail",
            div { class: "rail-head",
                div { class: "brand",
                    span { class: "tick" }
                    "weft"
                }
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
        }
    }
}
