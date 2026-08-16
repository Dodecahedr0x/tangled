use dioxus::prelude::*;
use weft_core::{EdgeKind, NetworkView};

use crate::rail::BandRail;
use crate::state::{banner_text, KnotStatus, Ui};

const D3_JS: Asset = asset!(
    "/assets/d3.min.js",
    AssetOptions::builder().with_hash_suffix(false)
);
const TOPO_JS: Asset = asset!(
    "/assets/topo-graph.js",
    AssetOptions::builder().with_hash_suffix(false)
);

#[component]
pub fn Topology() -> Element {
    let ui = try_use_context::<Signal<Ui>>().unwrap_or_else(|| Signal::new(Ui::empty()));
    rsx! {
        TopologyView {
            view: ui().view.clone(),
            knot_unreachable: ui().knot_status == KnotStatus::Unreachable,
            on_refresh: move |_| {
                spawn(async move { crate::state::sync_once(ui).await; });
            },
        }
    }
}

#[component]
pub fn TopologyView(
    view: Option<NetworkView>,
    knot_unreachable: bool,
    #[props(default)] on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        document::Script { src: D3_JS }
        document::Script { src: TOPO_JS }
        div { class: "shell",
            BandRail { show_refresh: true, on_refresh }
            section { class: "stage",
                if knot_unreachable {
                    div { class: "banner", {banner_text(KnotStatus::Unreachable).unwrap_or("Cannot reach knot")} }
                }
                p { class: "page-title",
                    span { class: "tick" }
                    "Topology"
                }
                if let Some(view) = view.as_ref() {
                    Graph { view: view.clone() }
                    ul { class: "topo-legend",
                        li { "Copper fill — configured knot" }
                        li { "Copper stroke — advertised public peers" }
                        li { "Small nodes — hosted public identities" }
                    }
                } else {
                    div { class: "empty",
                        span { class: "tick" }
                        p { "No graph yet" }
                        p { class: "muted", "Point Weft at a knot. The map is that knot plus its advertised peers." }
                    }
                }
            }
        }
    }
}

#[component]
fn Graph(view: NetworkView) -> Element {
    let payload = graph_json(&view);
    use_effect(|| {
        // ponytail: desktop/webview only; NoOpDocument on SSR
        document::eval(
            r#"
            (function () {
              if (typeof window === "undefined") return;
              function go() {
                var el = document.getElementById("weft-topo");
                if (el && window.weftTopoMount) window.weftTopoMount(el);
              }
              go();
              if (typeof d3 === "undefined" || !window.weftTopoMount) setTimeout(go, 80);
            })();
            "#,
        );
    });
    rsx! {
        div { class: "topo-body",
            div {
                id: "weft-topo",
                class: "topo-canvas",
                "data-graph": "{payload}",
                role: "img",
                aria_label: "Network topology",
                p { class: "muted topo-fallback",
                    "Interactive graph on desktop — drag nodes, scroll to zoom."
                }
            }
            aside { class: "topo-detail",
                p { class: "field-label", "Selection" }
                div { id: "weft-topo-detail",
                    p { class: "muted", "Hover or click a node." }
                }
            }
        }
    }
}

fn short_label(id: &str) -> String {
    if let Some(rest) = id.strip_prefix("did:sol:") {
        return format!(
            "did:…{}",
            rest.chars()
                .rev()
                .take(4)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
    }
    let t = id
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    if t.len() > 22 {
        format!("{}…", &t[..20])
    } else {
        t.to_string()
    }
}

fn graph_json(view: &NetworkView) -> String {
    let mut nodes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for k in &view.knots {
        let kind = if k.url == view.home { "home" } else { "peer" };
        let (identities, records, peers, commits) = match &k.stats {
            Some(s) => (
                Some(s.counts.identities),
                Some(s.counts.records),
                Some(s.counts.peers),
                Some(s.counts.commits),
            ),
            None => (None, None, None, None),
        };
        seen.insert(k.url.clone());
        nodes.push(serde_json::json!({
            "id": k.url,
            "kind": kind,
            "label": if k.did.is_empty() { short_label(&k.url) } else { short_label(&k.did) },
            "url": k.url,
            "did": k.did,
            "reachable": k.reachable,
            "identities": identities,
            "records": records,
            "peers": peers,
            "commits": commits,
        }));
    }
    for k in &view.knots {
        if let Some(s) = &k.stats {
            for h in &s.topology.hosted {
                if seen.insert(h.did.clone()) {
                    nodes.push(serde_json::json!({
                        "id": h.did,
                        "kind": "hosted",
                        "label": h.handle.clone().unwrap_or_else(|| short_label(&h.did)),
                        "did": h.did,
                        "handle": h.handle,
                        "hostedOn": k.url,
                        "records": h.records,
                    }));
                }
            }
        }
    }
    for e in &view.edges {
        if e.kind == EdgeKind::Hosted && seen.insert(e.to.clone()) {
            nodes.push(serde_json::json!({
                "id": e.to,
                "kind": "hosted",
                "label": short_label(&e.to),
                "did": e.to,
                "hostedOn": e.from,
            }));
        }
    }
    let links: Vec<serde_json::Value> = view
        .edges
        .iter()
        .map(|e| {
            serde_json::json!({
                "source": e.from,
                "target": e.to,
                "kind": match e.kind {
                    EdgeKind::Peer => "peer",
                    EdgeKind::Hosted => "hosted",
                },
            })
        })
        .collect();
    serde_json::json!({ "nodes": nodes, "links": links }).to_string()
}
