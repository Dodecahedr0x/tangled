use dioxus::prelude::*;
use loomed_core::LoomedKeys;

use crate::state::persist_keys;
use crate::{AppLink, Route};

#[component]
pub fn Onboard() -> Element {
    rsx! { OnboardView {} }
}

#[component]
pub fn OnboardView() -> Element {
    let mut did = use_signal(String::new);
    let mut json = use_signal(String::new);
    let mut err = use_signal(String::new);

    rsx! {
        div { class: "shell onboard",
            nav { class: "rail",
                div { class: "rail-head",
                    AppLink { to: Route::Inbox {}, class: "muted rail-link", "Inbox" }
                }
            }
            section { class: "stage",
                p { class: "brand", "Loomed" }
                p { class: "muted", "Generate a new identity, or import a key file." }
                button {
                    onclick: move |_| {
                        let keys = LoomedKeys::generate();
                        persist_keys(&keys);
                        did.set(keys.did().as_str());
                        err.set(String::new());
                    },
                    "generate"
                }
                textarea {
                    placeholder: "import json",
                    value: json,
                    oninput: move |e| json.set(e.value()),
                }
                button {
                    onclick: move |_| {
                        match LoomedKeys::from_solana_json(&json()) {
                            Ok(keys) => {
                                persist_keys(&keys);
                                did.set(keys.did().as_str());
                                err.set(String::new());
                            }
                            Err(e) => err.set(e.to_string()),
                        }
                    },
                    "import"
                }
                if !did().is_empty() {
                    p { class: "muted", {did()} }
                }
                if !err().is_empty() {
                    p { class: "banner", {err()} }
                }
                {wallet_slot(did())}
            }
        }
    }
}

fn wallet_slot(did: String) -> Element {
    let _ = did;
    rsx! {}
}
