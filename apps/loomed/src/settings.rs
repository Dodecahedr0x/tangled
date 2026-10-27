use dioxus::prelude::*;

use crate::state::{bootstrap, persist_knot, Ui};
use crate::{AppLink, Route};

#[component]
pub fn Settings() -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let initial = ui.as_ref().map(|s| s().knot).unwrap_or_default();
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
            nav { class: "rail",
                div { class: "rail-head",
                    AppLink { to: Route::Inbox {}, class: "muted rail-link", "Inbox" }
                }
            }
            section { class: "stage stage-pad",
                p { class: "brand", "Knot" }
                p { a { href: personal_url, "Open Personal" } }
                p { class: "muted", "Your library, notes, reading, recovery, and publishing. Start Personal on this computer to connect." }
                p { a { href: archive_url, "Your private archive & connections" } }
                p { class: "muted", "Browse imported conversations and media in your host account. Source participants are not invited to Tangled." }
                input {
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
