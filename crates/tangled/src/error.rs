//! Library error type. Recoverable failures return `Result`; none of these
//! paths panic.

use std::fmt;

/// Tangled client error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid CID: {0}")]
    InvalidCid(String),
    #[error("invalid DID: {0}")]
    InvalidDid(String),
    #[error("invalid signature")]
    InvalidSignature,
    #[error("DAG-CBOR: {0}")]
    DagCbor(String),
    #[error("capability: {0}")]
    Capability(String),
    #[error("page CRDT: {0}")]
    Page(String),
    #[error("MST: {0}")]
    Mst(String),
    #[error("HPKE: {0}")]
    Hpke(String),
    #[error("circle: {0}")]
    Circle(String),
    #[error("identity: {0}")]
    Identity(String),
    #[error("record: {0}")]
    Record(String),
    #[error("foreign: {0}")]
    Foreign(String),
    #[error("handle: {0}")]
    Handle(String),
    #[error("HTTP {status}: {error}")]
    Http { status: u16, error: String },
    #[error("HTTP transport: {0}")]
    Transport(String),
    #[error("codec: {0}")]
    Codec(String),
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub(crate) fn dagcbor(msg: impl fmt::Display) -> Self {
        Self::DagCbor(msg.to_string())
    }

    pub(crate) fn capability(msg: impl fmt::Display) -> Self {
        Self::Capability(msg.to_string())
    }

    pub(crate) fn page(msg: impl fmt::Display) -> Self {
        Self::Page(msg.to_string())
    }

    pub(crate) fn crypto(msg: impl fmt::Display) -> Self {
        Self::Crypto(msg.to_string())
    }
}

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;
