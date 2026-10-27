use dioxus::prelude::*;
use reed_core::Post;

use crate::post_row::PostRow;
use crate::rail::BandRail;
use crate::state::Ui;

#[component]
pub fn Search() -> Element {
    let mut ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    rsx! {
        SearchView {
            query: ui().query.clone(),
            hits: ui().hits(),
            self_did: ui().did.clone().unwrap_or_default(),
            on_query: move |q| { ui.write().query = q; },
        }
    }
}

#[component]
pub fn SearchView(
    query: String,
    hits: Vec<Post>,
    #[props(default)] self_did: String,
    #[props(default)] on_query: EventHandler<String>,
) -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let mut draft = use_signal(String::new);
    rsx! {
        div { class: "shell",
            BandRail {
                query: query.clone(),
                show_tune: true,
                self_did,
                on_query,
            }
            main { class: "stage", id: "main-content",
                crate::suite::SuiteBar { current: "Reed" }
                h1 { class: "page-title",
                    span { class: "tick" }
                    "Search"
                }
                form {
                    class: "follow-acct",
                    onsubmit: move |e| {
                        e.prevent_default();
                        let resource = draft();
                        if resource.trim().is_empty() {
                            return;
                        }
                        spawn(async move {
                            if crate::state::follow_resource(&resource).await.is_ok() {
                                if let Some(ui) = ui {
                                    crate::state::refresh_signal(ui);
                                }
                            }
                        });
                    },
                    input {
                        r#type: "text",
                        class: "tune",
                        placeholder: "acct:user@instance",
                        aria_label: "Follow acct or actor URL",
                        value: draft(),
                        oninput: move |e| draft.set(e.value()),
                    }
                    button { class: "follow-btn", r#type: "submit", "Follow" }
                }
                if hits.is_empty() {
                    div { class: "empty",
                        p { "No matches" }
                        p { class: "muted", "Search the local replica." }
                    }
                } else {
                    div { class: "log",
                        for p in hits.iter() {
                            PostRow { post: p.clone() }
                        }
                    }
                }
            }
        }
    }
}
