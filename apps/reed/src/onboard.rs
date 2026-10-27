use dioxus::prelude::*;
use reed_core::ReedKeys;

use crate::rail::BandRail;
use crate::state::{apply_onboard_keys, Ui};
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
    let ui = try_use_context::<Signal<Ui>>();
    let self_did = ui
        .map(|s| s().did.clone().unwrap_or_default())
        .unwrap_or_default();

    rsx! {
        div { class: "shell",
            BandRail { self_did }
            main { class: "stage", id: "main-content",
                crate::suite::SuiteBar { current: "Reed" }
                div { class: "stage-panel",
                    h1 { class: "page-title",
                        span { class: "tick" }
                        "Identity"
                    }
                    p { class: "muted", "Generate a new identity or import a key file. Keys stay on this device; your provider password does not restore them." }
                    p { class: "banner", "Generating or importing replaces the current local identity. Keep your existing key file before continuing." }
                    button {
                        onclick: move |_| {
                            let keys = ReedKeys::generate();
                            match ui {
                                Some(mut ui) => apply_onboard_keys(&mut ui.write(), &keys),
                                None => apply_onboard_keys(&mut Ui::empty(), &keys),
                            }
                            did.set(keys.did().as_str());
                            err.set(String::new());
                        },
                        "Generate identity"
                    }
                    div { class: "field",
                        label { class: "field-label", r#for: "import-json", "Key file JSON" }
                        textarea {
                            id: "import-json",
                            placeholder: "Paste key JSON",
                            value: json,
                            oninput: move |e| json.set(e.value()),
                        }
                        button {
                            onclick: move |_| {
                                match ReedKeys::from_solana_json(&json()) {
                                    Ok(keys) => {
                                        match ui {
                                            Some(mut ui) => apply_onboard_keys(&mut ui.write(), &keys),
                                            None => apply_onboard_keys(&mut Ui::empty(), &keys),
                                        }
                                        did.set(keys.did().as_str());
                                        err.set(String::new());
                                    }
                                    Err(e) => err.set(e.to_string()),
                                }
                            },
                            "Import"
                        }
                    }
                    if !did().is_empty() {
                        div { class: "onboard-done",
                            p { class: "did muted", {did()} }
                            AppLink { to: Route::Home {}, class: "muted", "Go to home" }
                        }
                    }
                    if !err().is_empty() {
                        p { class: "banner", {err()} }
                    }
                    {wallet_slot(did())}
                }
            }
        }
    }
}

fn wallet_slot(did: String) -> Element {
    let _ = did;
    rsx! {}
}
