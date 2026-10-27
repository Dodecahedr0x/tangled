use dioxus::prelude::*;

use crate::state::{bootstrap, persist_knot, Ui};
use crate::{AppLink, Route};

#[component]
pub fn Settings() -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let initial = ui.as_ref().map(|s| s().knot).unwrap_or_default();
    let archive_url = format!("{}/imports", initial.trim_end_matches('/'));
    let mut knot = use_signal(|| initial);
    rsx! {
        div { class: "shell",
            crate::rail::BandRail { show_band: true }
            main { class: "stage stage-pad", id: "main-content",
                crate::suite::SuiteBar { current: "Loomed" }
                AppLink { to: Route::Inbox {}, class: "back-link", "Back to Inbox" }
                h1 { class: "page-title", "Host settings" }
                p { a { href: archive_url, "Your private archive & connections" } }
                p { class: "muted", "Browse imported conversations and media in your host account. Source participants are not invited to Tangled." }
                label { r#for: "host-url", "Host URL" }
                input {
                    id: "host-url",
                    r#type: "url",
                    placeholder: "https://knot.example",
                    value: knot,
                    oninput: move |e| knot.set(e.value()),
                }
                button {
                    onclick: move |_| {
                        persist_knot(&knot());
                        if let Some(ui) = ui {
                            spawn(async move {
                                bootstrap(ui).await;
                            });
                        }
                    },
                    "Save"
                }
            }
        }
    }
}
