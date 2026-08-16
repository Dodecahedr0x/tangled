//! Reed domain crate (no Dioxus). Protocol via `tangled` only.

pub mod error;
pub mod graph;
pub mod handle;
pub mod identity;
pub mod outbox;
pub mod post;
pub mod replica;
pub mod search;
pub mod seed;
pub mod social;
pub mod sync;
pub mod types;

mod wire;

pub use error::{ReedError, Result};
pub use graph::{block, follow, unblock, unfollow};
pub use handle::{is_ap_resource, parse_peer, resolve_handle, resolve_peer};
pub use identity::{FileKeyStore, KeyStore, ReedKeys};
pub use outbox::Outbox;
pub use post::PostApi;
pub use replica::Replica;
pub use search::{mentions, search};
pub use social::{boost, like, mute, put_profile, unboost, unlike, unmute};
pub use sync::Sync;
pub use tangled::Did;
pub use types::{
    KnotUrl, MediaRef, Peer, Post, PostId, Profile, SocialEvent, SocialKind, TimelineItem,
    TimelineReason,
};
