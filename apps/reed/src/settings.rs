use dioxus::prelude::*;

use crate::rail::BandRail;
use crate::state::{persist_knot, probe_knot, Ui, DEFAULT_KNOT_URL};

#[component]
pub fn Settings() -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let initial = ui.map(|s| s().knot).unwrap_or_default();
    let self_did = ui
        .map(|s| s().did.clone().unwrap_or_default())
        .unwrap_or_default();
    let archive_url = format!("{}/imports", initial.trim_end_matches('/'));
    #[cfg(not(target_arch = "wasm32"))]
    let personal_url = std::env::var("TANGLED_PERSONAL_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            option_env!("TANGLED_PERSONAL_URL")
                .unwrap_or("http://127.0.0.1:8788/")
                .to_owned()
        });
    #[cfg(target_arch = "wasm32")]
    let personal_url = option_env!("TANGLED_PERSONAL_URL").unwrap_or("http://127.0.0.1:8788/");
    let mut knot = use_signal(|| initial);
    rsx! {
        div { class: "shell",
            BandRail {
                self_did,
                show_refresh: true,
                on_refresh: move |_| {
                    if let Some(ui) = ui {
                        spawn(async move { crate::state::sync_once(ui).await; });
                    }
                },
            }
            section { class: "stage",
                div { class: "stage-panel",
                    p { class: "page-title",
                        span { class: "tick" }
                        "Settings"
                    }
                    p { a { href: personal_url, "Open Personal" } }
                    p { class: "muted", "Your library, notes, reading, recovery, and publishing. Start Personal on this computer to connect." }
                    p { class: "muted", "Knot URL — the node Reed syncs with" }
                    p { a { href: archive_url, "Your private archive & connections" } }
                    p { class: "muted", "Import posts, conversations, and media from other apps in your host account. Imported history stays private." }
                    div { class: "field",
                        label { class: "field-label", r#for: "knot-url", "Knot URL" }
                        div { class: "control-row",
                            input {
                                r#type: "url",
                                id: "knot-url",
                                placeholder: DEFAULT_KNOT_URL,
                                value: knot,
                                oninput: move |e| knot.set(e.value()),
                            }
                            button {
                                onclick: move |_| {
                                    persist_knot(&knot());
                                    if let Some(mut ui) = ui {
                                        ui.write().knot = knot();
                                        spawn(async move {
                                            let status = probe_knot(&knot()).await;
                                            ui.write().knot_status = status;
                                        });
                                    }
                                },
                                "Save"
                            }
                        }
                    }
                }
            }
        }
    }
}
