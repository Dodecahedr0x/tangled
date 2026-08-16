use dioxus::prelude::*;
use weft_core::LookupHit;

use crate::rail::BandRail;
use crate::state::{banner_text, KnotStatus, Ui};

#[component]
pub fn Lookup() -> Element {
    let mut ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    rsx! {
        LookupView {
            query: ui().lookup_q.clone(),
            hit: ui().lookup.clone(),
            err: ui().lookup_err.clone(),
            knot_unreachable: ui().knot_status == KnotStatus::Unreachable,
            on_query: move |q| { ui.write().lookup_q = q; },
            on_go: move |_| {
                spawn(async move { crate::state::run_lookup(ui).await; });
            },
        }
    }
}

#[component]
pub fn LookupView(
    query: String,
    hit: Option<LookupHit>,
    err: Option<String>,
    knot_unreachable: bool,
    #[props(default)] on_query: EventHandler<String>,
    #[props(default)] on_go: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "shell",
            BandRail {}
            section { class: "stage",
                if knot_unreachable {
                    div { class: "banner", {banner_text(KnotStatus::Unreachable).unwrap_or("Cannot reach knot")} }
                }
                p { class: "page-title",
                    span { class: "tick" }
                    "Lookup"
                }
                div { class: "stage-panel",
                    p { class: "muted", "DID, handle, or knot URL" }
                    div { class: "control-row",
                        input {
                            r#type: "search",
                            class: "tune",
                            placeholder: "did:sol:… or alice.sol or http://127.0.0.1:3000",
                            aria_label: "Lookup identity or knot",
                            value: query,
                            oninput: move |e| on_query.call(e.value()),
                        }
                        button { r#type: "button", onclick: move |_| on_go.call(()), "Go" }
                    }
                    if let Some(err) = err {
                        p { class: "muted", "{err}" }
                    }
                    if let Some(hit) = hit {
                        HitCard { hit }
                    }
                }
            }
        }
    }
}

#[component]
fn HitCard(hit: LookupHit) -> Element {
    match hit {
        LookupHit::Knot { url, stats } => rsx! {
            div { class: "hit",
                p { class: "page-title",
                    span { class: "tick" }
                    "Knot"
                }
                p { class: "did", "{url}" }
                p { class: "did", "{stats.did}" }
                p { class: "muted",
                    "{stats.counts.identities} identities · {stats.counts.records} records · {stats.counts.peers} peers"
                }
            }
        },
        LookupHit::Identity {
            did,
            handle,
            knot,
            rev,
            records,
        } => rsx! {
            div { class: "hit",
                p { class: "page-title",
                    span { class: "tick" }
                    "Identity"
                }
                p { class: "did", "{did}" }
                if let Some(h) = handle {
                    p { class: "muted", "{h}" }
                }
                if let Some(k) = knot {
                    p { class: "muted", "knot {k}" }
                }
                if let Some(r) = rev {
                    p { class: "muted", "rev {r}" }
                }
                if let Some(n) = records {
                    p { class: "muted", "{n} public records on this knot" }
                }
            }
        },
    }
}
