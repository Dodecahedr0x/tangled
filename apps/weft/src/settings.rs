use dioxus::prelude::*;

use crate::rail::BandRail;
use crate::state::{persist_knot, probe_knot, Ui, DEFAULT_KNOT_URL};

#[component]
pub fn Settings() -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let initial = ui.map(|s| s().knot).unwrap_or_default();
    let personal_url = option_env!("TANGLED_PERSONAL_URL").unwrap_or("http://127.0.0.1:8788/");
    let mut knot = use_signal(|| initial);
    rsx! {
        div { class: "shell",
            BandRail {}
            section { class: "stage",
                div { class: "stage-panel",
                    p { class: "page-title",
                        span { class: "tick" }
                        "Settings"
                    }
                    p { a { href: personal_url, "Open Personal" } }
                    p { class: "muted", "Your library, notes, reading, recovery, and publishing. Start Personal on this computer to connect." }
                    p { class: "muted", "Knot URL — Weft reads node.stats from this node and crawls its advertised peers." }
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
                                            crate::state::sync_once(ui).await;
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
