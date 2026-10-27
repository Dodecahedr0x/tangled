//! Reed Dioxus UI. Domain lives in `reed-core`.

use dioxus::prelude::*;

mod avatar;
mod compose;
mod discover;
mod home;
mod icons;
mod notify;
mod onboard;
mod paths;
mod post_row;
mod profile;
mod rail;
mod search_page;
mod settings;
mod state;
#[path = "../../suite.rs"]
mod suite;
mod thread;

pub use compose::{Compose, ComposeView};
pub use discover::{Discover, DiscoverView};
pub use home::{Home, HomeView};
pub use onboard::{Onboard, OnboardView};
pub use profile::{Profile, ProfileView};
pub use rail::BandRail;
pub use search_page::{Search, SearchView};
pub use settings::Settings;
pub use thread::{Thread, ThreadView};

/// Router `Link` when a router is present; `a { href }` for SSR harnesses without one.
#[component]
pub fn AppLink(
    to: Route,
    #[props(default)] class: String,
    #[props(default)] aria_label: String,
    children: Element,
) -> Element {
    let href = to.to_string();
    let router = try_consume_context::<dioxus::prelude::dioxus_router::RouterContext>();
    let current = router.as_ref().is_some_and(|r| r.current::<Route>() == to);
    let active = if current { Some("page") } else { None };
    let aria_label = (!aria_label.is_empty()).then_some(aria_label);
    if router.is_some() {
        rsx! { Link { to, class, aria_current: active, aria_label, {children} } }
    } else {
        rsx! { a { href, class, aria_current: active, aria_label, {children} } }
    }
}

const FIRA_SANS_REGULAR: Asset = asset!(
    "/assets/fonts/FiraSans-Regular.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);

const MPLUS_CODE: Asset = asset!(
    "/assets/fonts/MPLUS1Code-Regular.woff2",
    AssetOptions::builder().with_hash_suffix(false)
);

pub const APP_CSS: &str = include_str!("../assets/app.css");

fn app_style() -> String {
    format!("{}\n{}", include_str!("../../commons.css"), APP_CSS)
        .replace(
            "url(\"./fonts/FiraSans-Regular.woff2\")",
            &format!(
                "url(\"{}\")",
                hosted_font(
                    &FIRA_SANS_REGULAR.to_string(),
                    "/fonts/FiraSans-Regular.woff2"
                )
            ),
        )
        .replace(
            "url(\"./fonts/MPLUS1Code-Regular.woff2\")",
            &format!(
                "url(\"{}\")",
                hosted_font(&MPLUS_CODE.to_string(), "/fonts/MPLUS1Code-Regular.woff2")
            ),
        )
}

/// `dx` rewrites the asset URL. A plain `cargo` wasm build leaves the placeholder,
/// so the hosted site serves the file from `/fonts`.
fn hosted_font(asset: &str, hosted: &str) -> String {
    if asset.contains("replaced by dx") {
        hosted.to_string()
    } else {
        asset.to_string()
    }
}

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/d")]
    Discover {},
    #[route("/p/:did")]
    Profile { did: String },
    #[route("/t/:cid")]
    Thread { cid: String },
    #[route("/compose")]
    Compose {},
    #[route("/search")]
    Search {},
    #[route("/settings")]
    Settings {},
    #[route("/onboard")]
    Onboard {},
}

#[component]
pub fn App() -> Element {
    crate::notify::ensure_macos_application();
    let ui = use_context_provider(|| Signal::new(state::Ui::load()));
    use_future(move || async move {
        crate::state::sync_once(ui).await;
    });
    rsx! {
        style { {app_style()} }
        a { href: "#main-content", class: "skip-link", "Skip to main content" }
        Router::<Route> {}
    }
}

#[cfg(test)]
mod ssr_tests;
