use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MirrorVaultError>;

#[derive(Debug, Error)]
pub enum MirrorVaultError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid bundle at {path}: {message}")]
    InvalidBundle { path: PathBuf, message: String },

    #[error("unsupported source shape: {0}")]
    UnsupportedSource(String),

    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("restore invariant failed: {0}")]
    RestoreInvariant(String),

    #[error("mirror command failed: {0}")]
    Git(String),

    #[error("integrity check failed: expected {expected}, actual {actual}")]
    Integrity { expected: String, actual: String },
}

impl MirrorVaultError {
    pub fn invalid_bundle(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::InvalidBundle {
            path: path.into(),
            message: message.into(),
        }
    }
}
