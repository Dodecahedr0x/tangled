use dioxus::prelude::*;

use crate::state::{bootstrap, persist_knot, Ui};
use crate::{AppLink, Route};

#[component]
pub fn Settings() -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let initial = ui.as_ref().map(|s| s().knot).unwrap_or_default();
    let mut knot = use_signal(|| initial);
    rsx! {
        div { class: "shell",
            nav { class: "rail",
                div { class: "rail-head",
                    AppLink { to: Route::Inbox {}, class: "muted rail-link", "band" }
                }
            }
            section { class: "stage stage-pad",
                p { class: "brand", "knot" }
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
                    "save"
                }
            }
        }
    }
}
