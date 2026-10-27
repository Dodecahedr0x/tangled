//! Loomed Dioxus UI. Domain lives in `loomed-core`.

use dioxus::prelude::*;

mod compose;
mod inbox;
mod labels;
mod notify;
mod onboard;
mod paths;
mod settings;
mod state;
mod thread;

pub use compose::Compose;
pub use inbox::{Inbox, InboxView};
pub use onboard::{Onboard, OnboardView};
pub use settings::Settings;
pub use thread::{Thread, ThreadView};

/// Router `Link` when a router is present; `a { href }` for SSR harnesses without one.
#[component]
pub fn AppLink(to: Route, #[props(default)] class: String, children: Element) -> Element {
    let href = to.to_string();
    if try_consume_context::<dioxus::prelude::dioxus_router::RouterContext>().is_some() {
        rsx! {
            Link { to, class, {children} }
        }
    } else {
        rsx! {
            a { href, class, {children} }
        }
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
    APP_CSS
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
    Inbox {},
    #[route("/c/:circle")]
    Thread { circle: String },
    #[route("/new")]
    Compose {},
    #[route("/settings")]
    Settings {},
    #[route("/onboard")]
    Onboard {},
}

#[component]
pub fn App() -> Element {
    crate::notify::ensure_macos_application();
    let ui = use_context_provider(|| Signal::new(state::Ui::load()));
    #[cfg(any(feature = "desktop", target_arch = "wasm32"))]
    {
        use_future(move || async move {
            state::bootstrap(ui).await;
        });
    }
    #[cfg(not(any(feature = "desktop", target_arch = "wasm32")))]
    let _ = ui;
    rsx! {
        style { {app_style()} }
        Router::<Route> {}
    }
}

#[cfg(test)]
mod ssr_tests;
