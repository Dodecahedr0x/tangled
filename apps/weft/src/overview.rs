use dioxus::prelude::*;
use weft_core::NetworkView;

use crate::rail::BandRail;
use crate::state::{banner_text, KnotStatus, Ui};

#[component]
pub fn Overview() -> Element {
    let ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    rsx! {
        OverviewView {
            view: ui().view.clone(),
            knot_unreachable: ui().knot_status == KnotStatus::Unreachable,
            knot_unconfigured: ui().knot_status == KnotStatus::Unconfigured,
            on_refresh: move |_| {
                spawn(async move { crate::state::sync_once(ui).await; });
            },
        }
    }
}

#[component]
pub fn OverviewView(
    view: Option<NetworkView>,
    knot_unreachable: bool,
    #[props(default)] knot_unconfigured: bool,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "shell",
            BandRail { show_refresh: true, on_refresh }
            section { class: "stage",
                div { class: "band",
                    if knot_unconfigured {
                        span { class: "banner", {banner_text(KnotStatus::Unconfigured).unwrap_or("No knot URL set")} }
                    } else if knot_unreachable {
                        span { class: "banner late", {banner_text(KnotStatus::Unreachable).unwrap_or("Cannot reach knot")} }
                    }
                }
                p { class: "page-title",
                    span { class: "tick" }
                    "Overview"
                }
                if let Some(view) = view.as_ref() {
                    div { class: "stat-grid",
                        Stat { label: "Knots", value: view.totals.knots.to_string() }
                        Stat { label: "Identities", value: view.totals.identities.to_string() }
                        Stat { label: "Records", value: view.totals.records.to_string() }
                        Stat { label: "Blobs", value: view.totals.blobs.to_string() }
                        Stat { label: "Circles", value: view.totals.circles.to_string() }
                        Stat { label: "Foreign", value: view.totals.foreign.to_string() }
                        Stat { label: "Commits", value: view.totals.commits.to_string() }
                    }
                    if let Some(home) = view.knots.iter().find(|n| n.url == view.home) {
                        if let Some(s) = home.stats.as_ref() {
                            p { class: "muted stat-note",
                                "Home knot {s.did} · retained public commits {s.rates.public_commits}/{s.rates.retained}"
                            }
                        }
                    }
                } else if !knot_unconfigured && !knot_unreachable {
                    div { class: "empty",
                        span { class: "tick" }
                        p { "No stats yet" }
                        p { class: "muted", "Refresh to pull node.stats from the configured knot." }
                    }
                }
            }
        }
    }
}

#[component]
fn Stat(label: String, value: String) -> Element {
    rsx! {
        div { class: "stat",
            p { class: "stat-value", "{value}" }
            p { class: "stat-label", "{label}" }
        }
    }
}
