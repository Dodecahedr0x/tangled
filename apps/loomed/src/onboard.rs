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
    let ui = try_use_context::<Signal<crate::state::Ui>>();
    let mut did = use_signal(String::new);
    let mut json = use_signal(String::new);
    let mut err = use_signal(String::new);

    rsx! {
        div { class: "shell onboard",
            crate::rail::BandRail { show_band: true }
            main { class: "stage", id: "main-content",
                crate::suite::SuiteBar { current: "Loomed" }
                AppLink { to: Route::Inbox {}, class: "back-link", "Back to Inbox" }
                h1 { class: "page-title", "Identity" }
                p { class: "muted", "Keys on this device. Your provider account has a separate identity." }
                p { class: "muted", "Generate a new identity, or import a key file." }
                p { class: "banner", "Generating or importing replaces the current local identity. Keep your existing key file before continuing." }
                button {
                    onclick: move |_| {
                        let keys = LoomedKeys::generate();
                        if let Some(mut ui) = ui {
                            crate::state::apply_onboard_keys(&mut ui.write(), &keys);
                        } else {
                            persist_keys(&keys);
                        }
                        did.set(keys.did().as_str());
                        err.set(String::new());
                    },
                    "Generate identity"
                }
                label { r#for: "import-identity", "Key file JSON" }
                textarea {
                    id: "import-identity",
                    placeholder: "import json",
                    value: json,
                    oninput: move |e| json.set(e.value()),
                }
                button {
                    onclick: move |_| {
                        match LoomedKeys::from_solana_json(&json()) {
                            Ok(keys) => {
                                if let Some(mut ui) = ui {
                                    crate::state::apply_onboard_keys(&mut ui.write(), &keys);
                                } else {
                                    persist_keys(&keys);
                                }
                                did.set(keys.did().as_str());
                                err.set(String::new());
                            }
                            Err(e) => err.set(e.to_string()),
                        }
                    },
                    "Import identity"
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
