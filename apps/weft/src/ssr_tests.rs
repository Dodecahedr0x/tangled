use dioxus::prelude::*;
use weft_core::{
    EdgeKind, HostedIdentity, KnotNode, LookupHit, NetworkTotals, NetworkView, NodeCounts,
    NodeRates, NodeStats, NodeTopology, TopoEdge,
};

use crate::{LookupView, OverviewView, TopologyView};

fn html(root: fn() -> Element) -> String {
    let mut vdom = VirtualDom::new(root);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn sample_stats() -> NodeStats {
    NodeStats {
        v: 1,
        did: "did:sol:alice".into(),
        as_of: 1_700_000_000,
        counts: NodeCounts {
            identities: 1,
            repos: 1,
            records: 4,
            blobs: 0,
            peers: 1,
            circles: 0,
            foreign: 0,
            commits: 4,
        },
        rates: NodeRates {
            public_commits: 4,
            retained: 256,
        },
        topology: NodeTopology {
            peers: vec!["https://b.example".into()],
            hosted: vec![HostedIdentity {
                did: "did:sol:alice".into(),
                handle: Some("alice.sol".into()),
                rev: Some("3jzf".into()),
                records: 4,
            }],
            truncated: false,
        },
    }
}

fn sample_view() -> NetworkView {
    let stats = sample_stats();
    NetworkView {
        home: "http://127.0.0.1:3000".into(),
        knots: vec![
            KnotNode {
                url: "http://127.0.0.1:3000".into(),
                did: stats.did.clone(),
                reachable: true,
                stats: Some(stats),
            },
            KnotNode {
                url: "https://b.example".into(),
                did: String::new(),
                reachable: false,
                stats: None,
            },
        ],
        edges: vec![
            TopoEdge {
                from: "http://127.0.0.1:3000".into(),
                to: "https://b.example".into(),
                kind: EdgeKind::Peer,
            },
            TopoEdge {
                from: "http://127.0.0.1:3000".into(),
                to: "did:sol:alice".into(),
                kind: EdgeKind::Hosted,
            },
        ],
        totals: NetworkTotals {
            knots: 2,
            identities: 1,
            records: 4,
            blobs: 0,
            circles: 0,
            foreign: 0,
            commits: 4,
        },
    }
}

fn empty_overview() -> Element {
    rsx! {
        OverviewView {
            view: None,
            knot_unreachable: false,
            knot_unconfigured: true,
        }
    }
}

fn unreachable_overview() -> Element {
    rsx! {
        OverviewView {
            view: None,
            knot_unreachable: true,
        }
    }
}

fn filled_overview() -> Element {
    rsx! {
        OverviewView {
            view: Some(sample_view()),
            knot_unreachable: false,
        }
    }
}

fn empty_topo() -> Element {
    rsx! { TopologyView { view: None, knot_unreachable: false } }
}

fn filled_topo() -> Element {
    rsx! {
        TopologyView {
            view: Some(sample_view()),
            knot_unreachable: false,
        }
    }
}

fn lookup_identity() -> Element {
    rsx! {
        LookupView {
            query: "did:sol:alice",
            hit: Some(LookupHit::Identity {
                did: "did:sol:alice".into(),
                handle: Some("alice.sol".into()),
                knot: Some("http://127.0.0.1:3000".into()),
                rev: Some("3jzf".into()),
                records: Some(4),
            }),
            err: None,
            knot_unreachable: false,
        }
    }
}

#[test]
fn overview_empty_mentions_weft_and_no_stats() {
    let h = html(empty_overview);
    assert!(h.to_ascii_lowercase().contains("weft"));
    assert!(h.contains("Overview"));
    assert!(h.contains("No knot URL set"));
}

#[test]
fn overview_unreachable_banner() {
    let h = html(unreachable_overview);
    assert!(h.contains("Cannot reach knot"));
}

#[test]
fn overview_shows_network_counts() {
    let h = html(filled_overview);
    assert!(h.contains("Identities"));
    assert!(h.contains(">4<") || h.contains("4"));
    assert!(h.contains("Records"));
    assert!(h.contains("retained public commits"));
}

#[test]
fn topology_renders_interactive_mount() {
    let h = html(filled_topo);
    assert!(h.contains("Topology"));
    assert!(h.contains("weft-topo"));
    assert!(h.contains("topo-canvas"));
    assert!(h.contains("data-graph"));
    assert!(h.contains("127.0.0.1:3000"));
    assert!(h.contains("b.example"));
    assert!(h.contains("did:sol:alice"));
    assert!(h.contains("topo-detail"));
    assert!(h.contains("Hover or click"));
}

#[test]
fn topology_empty_explains_graph() {
    let h = html(empty_topo);
    assert!(h.contains("No graph yet"));
}

#[test]
fn lookup_shows_identity_hit() {
    let h = html(lookup_identity);
    assert!(h.contains("did:sol:alice"));
    assert!(h.contains("alice.sol"));
    assert!(h.contains("public records"));
}

#[test]
fn commons_overview_explains_observed_public_scope() {
    let s = html(filled_overview);
    assert!(s.contains("<h1"), "overview needs a real heading: {s}");
    assert!(
        s.contains("advertised public peers"),
        "never imply global counts: {s}"
    );
    assert!(
        s.contains("Read-only"),
        "explorer must not imply write access: {s}"
    );
}

#[test]
fn capture_commons_weft_views() {
    let Ok(dir) = std::env::var("TANGLED_DESIGN_CAPTURE_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for (name, root) in [
        ("weft-overview", filled_overview as fn() -> Element),
        ("weft-topology", filled_topo),
        ("weft-lookup", lookup_identity),
        ("weft-empty", empty_overview),
    ] {
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.html")), format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{name}</title><style>{}</style><p class=\"fixture-note\">Synthetic test data · server-rendered Rust view</p>{}</html>", crate::app_style(), html(root))).unwrap();
    }
}

#[test]
fn topology_has_keyboard_accessible_public_node_details() {
    let s = html(filled_topo);
    assert!(
        s.contains("<details"),
        "public nodes need keyboard-accessible details: {s}"
    );
    assert!(
        s.contains("<summary"),
        "each node needs a selectable label: {s}"
    );
    assert!(
        s.contains("Public nodes"),
        "text view must be discoverable: {s}"
    );
}
