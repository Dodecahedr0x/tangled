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
mod thread;

#[cfg(target_arch = "wasm32")]
mod wallet;

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
    #[cfg(target_arch = "wasm32")]
    {
        rsx! {
            wallet::WithWallet {
                style { {app_style()} }
                Router::<Route> {}
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! {
            style { {app_style()} }
            Router::<Route> {}
        }
    }
}

#[cfg(test)]
mod ssr_tests;
