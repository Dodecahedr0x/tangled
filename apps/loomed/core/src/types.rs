use serde::{Deserialize, Serialize};
use tangled::Did;

/// Circle id: `did:sol:<owner>#<rkey>`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CircleRef(pub String);

/// Record CID as a string.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KnotUrl(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    pub circle: CircleRef,
    pub title: String,
    pub last_preview: String,
    pub unread: u32,
    /// Circle member DIDs (owner + peers). Used to label the inbox row.
    #[serde(default)]
    pub members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub circle: CircleRef,
    pub author: String,
    pub text: Option<String>,
    pub blob_cid: Option<String>,
    pub ts_us: u64,
    pub receipts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Peer {
    Did(Did),
    Handle(String),
}
