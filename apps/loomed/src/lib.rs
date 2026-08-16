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

#[cfg(target_arch = "wasm32")]
mod wallet;

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
    let ui = use_context_provider(|| Signal::new(state::Ui::load()));
    #[cfg(feature = "desktop")]
    {
        use_future(move || async move {
            state::bootstrap(ui).await;
        });
    }
    #[cfg(not(feature = "desktop"))]
    let _ = ui;
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
