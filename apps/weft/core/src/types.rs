use serde::{Deserialize, Serialize};
use tangled::http::NodeStats;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KnotUrl(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnotNode {
    pub url: String,
    pub did: String,
    pub reachable: bool,
    pub stats: Option<NodeStats>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    Peer,
    Hosted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopoEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkTotals {
    pub knots: u64,
    pub identities: u64,
    pub records: u64,
    pub blobs: u64,
    pub circles: u64,
    pub foreign: u64,
    pub commits: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkView {
    pub home: String,
    pub knots: Vec<KnotNode>,
    pub edges: Vec<TopoEdge>,
    pub totals: NetworkTotals,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupHit {
    Knot {
        url: String,
        stats: NodeStats,
    },
    Identity {
        did: String,
        handle: Option<String>,
        knot: Option<String>,
        rev: Option<String>,
        records: Option<u64>,
    },
}
