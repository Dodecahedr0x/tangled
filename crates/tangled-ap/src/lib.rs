//! ActivityPub mapping + HTTP signatures for the tangled-knot gateway.
//!
//! Used only by `tangled-knot` (same process). Reed stays a Tangled client.

pub mod activity;
pub mod actor;
#[cfg(feature = "demo")]
pub mod demo;
pub mod error;
pub mod httpsig;
pub mod note;
pub mod outbox;
pub mod webfinger;

pub use activity::{
    accept_follow, announce_activity, follow_activity, like_activity, parse_inbound, Inbound,
};
pub use actor::{actor_to_profile, person_document, MappedProfile};
pub use error::{Error, Result};
pub use httpsig::{sign_header_list, sign_request, verify_request, verify_request_at};
pub use note::{create_note, note_to_post, post_to_note, MappedPost};
pub use outbox::{
    first_page_of, inbound_from_outbox_item, parse_outbox_collection, parse_outbox_page, FirstPage,
    OutboxIndex,
};
pub use webfinger::{build_jrd, parse_jrd, WebFinger};
