use tangled::Error as ProtocolError;

#[derive(Debug, thiserror::Error)]
pub enum WeftError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("io: {0}")]
    Io(String),
    #[error("knot unreachable")]
    KnotUnreachable,
    #[error("not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, WeftError>;

impl From<std::io::Error> for WeftError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
