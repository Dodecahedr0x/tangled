//! Loomed domain crate (no Dioxus). Protocol via `tangled` only.

pub mod chat;
pub mod error;
pub mod handle;
pub mod identity;
pub mod outbox;
pub mod receipts;
pub mod replica;
pub mod search;
pub mod seed;
pub mod sync;
pub mod types;

mod envelope;

pub use chat::Chat;
pub use error::{LoomedError, Result};
pub use handle::parse_peer;
pub use identity::{FileKeyStore, KeyStore, LoomedKeys};
pub use outbox::Outbox;
pub use receipts::{is_receipt, seal_receipt};
pub use replica::Replica;
pub use search::search;
pub use sync::Sync;
pub use types::{CircleRef, Conversation, KnotUrl, Message, MessageId, Peer};
