use tangled::Error as ProtocolError;

#[derive(Debug, thiserror::Error)]
pub enum ReedError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("io: {0}")]
    Io(String),
    #[error("offline")]
    Offline,
    #[error("knot unreachable")]
    KnotUnreachable,
    #[error("identity: {0}")]
    Identity(String),
    #[error("handle: {0}")]
    Handle(String),
    #[error("codec: {0}")]
    Codec(String),
    #[error("media too large")]
    MediaTooLarge,
    #[error("media limit")]
    MediaLimit,
}

pub type Result<T> = std::result::Result<T, ReedError>;

impl From<std::io::Error> for ReedError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
