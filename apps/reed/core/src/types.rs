use serde::{Deserialize, Serialize};
use tangled::Did;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PostId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KnotUrl(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaRef {
    pub cid: String,
    pub mime: String,
    pub alt: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Post {
    pub id: PostId,
    pub rkey: String,
    pub author: String,
    pub handle: Option<String>,
    pub text: String,
    pub langs: Vec<String>,
    pub reply_to: Option<PostId>,
    pub quote_of: Option<PostId>,
    pub media: Vec<MediaRef>,
    pub ts_us: u64,
    pub like_count: u32,
    pub boost_count: u32,
    pub liked_by_me: bool,
    pub boosted_by_me: bool,
    pub deleted: bool,
    #[serde(default)]
    pub attested: bool,
    /// Original AP object URL (Note `id`) or actor URL when federated.
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineReason {
    Authored,
    Boost { by: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimelineItem {
    pub post: Post,
    pub reason: TimelineReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialKind {
    Follow,
    Like,
    Boost,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialEvent {
    pub id: String,
    pub actor: String,
    pub kind: SocialKind,
    pub target: Option<PostId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub did: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_cid: Option<String>,
    /// Canonical AP actor URL when this DID is a federated import.
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Peer {
    Did(Did),
    Handle(String),
}
