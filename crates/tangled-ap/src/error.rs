use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Protocol(String),
    #[error("http signature: {0}")]
    Signature(String),
    #[error(transparent)]
    Foreign(#[from] tangled::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
