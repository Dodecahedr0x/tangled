use tangled::Error as ProtocolError;

#[derive(Debug, thiserror::Error)]
pub enum LoomedError {
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
}

pub type Result<T> = std::result::Result<T, LoomedError>;

impl From<std::io::Error> for LoomedError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
