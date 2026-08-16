//! Weft Dioxus UI. Domain lives in `weft-core`.

use dioxus::prelude::*;

mod icons;
mod lookup;
mod overview;
mod paths;
mod rail;
mod settings;
mod state;
mod topology;

pub use lookup::{Lookup, LookupView};
pub use overview::{Overview, OverviewView};
pub use rail::BandRail;
pub use settings::Settings;
pub use topology::{Topology, TopologyView};

/// Router `Link` when a router is present; `a { href }` for SSR harnesses without one.
#[component]
pub fn AppLink(
    to: Route,
    #[props(default)] class: String,
    #[props(default)] aria_label: String,
    children: Element,
) -> Element {
    let href = to.to_string();
    let routed = try_consume_context::<dioxus::prelude::dioxus_router::RouterContext>().is_some();
    if routed && !aria_label.is_empty() {
        rsx! { Link { to, class, aria_label, {children} } }
    } else if routed {
        rsx! { Link { to, class, {children} } }
    } else if !aria_label.is_empty() {
        rsx! { a { href, class, aria_label, {children} } }
    } else {
        rsx! { a { href, class, {children} } }
    }
}

const FIRA_SANS_REGULAR: Asset = asset!(
    "/assets/fonts/FiraSans-Regular.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);

pub const APP_CSS: &str = include_str!("../assets/app.css");

fn app_style() -> String {
    APP_CSS.replace(
        "url(\"./fonts/FiraSans-Regular.woff2\")",
        &format!("url(\"{FIRA_SANS_REGULAR}\")"),
    )
}

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[route("/")]
    Overview {},
    #[route("/topo")]
    Topology {},
    #[route("/lookup")]
    Lookup {},
    #[route("/settings")]
    Settings {},
}

#[component]
pub fn App() -> Element {
    let ui = use_context_provider(|| Signal::new(state::Ui::load()));
    use_future(move || async move {
        crate::state::sync_once(ui).await;
    });
    rsx! {
        style { {app_style()} }
        Router::<Route> {}
    }
}

#[cfg(test)]
mod ssr_tests;
