//! Tangled protocol reference client.
//!
//! Identity, Solana off-chain signatures, DAG-CBOR + BLAKE3 CIDs, MST proofs,
//! capabilities, page CRDT, circle dual-stack, HPKE, MLS (RFC 9420), SIWS sessions,
//! handle resolution, and a typed knot HTTP client.

pub mod capability;
pub mod car;
pub mod cid;
pub mod circle;
pub mod constants;
pub mod dagcbor;
pub mod error;
pub mod foreign;
pub mod handle;
pub mod hpke;
pub mod http;
pub mod httpsig;
pub mod identity;
pub mod knot;
pub mod mls;
pub mod mst;
pub mod page;
pub mod record;
pub mod recovery;
pub mod session;
pub mod sign;
pub mod sync;

pub use capability::{
    attenuates, authorizes, resource_attenuates, verify_circle_read, Capability, Invocation,
    Resource,
};
pub use car::Car;
pub use cid::Cid;
pub use circle::{CircleState, Distributor, MembershipPlan};
pub use error::{Error, Result};
pub use http::{KnotClient, OverlayCount};
pub use identity::{Did, Identity};
pub use knot::{KnotEngine, MockKnot};
pub use mst::Mst;
pub use page::{apply as apply_page, LivePage, PageRecord};
pub use record::Visibility;
pub use sync::{getblob_auth, BlobAuth, BlobRef, HeadCache};
