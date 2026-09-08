use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, JeryuCacheError>;

#[derive(Debug, Error)]
pub enum JeryuCacheError {
    #[error("invalid digest: {0}")]
    InvalidDigest(String),
    #[error("cache law denied request: {0}")]
    PolicyDenied(String),
    #[error("cache miss for digest {0}")]
    CacheMiss(String),
    #[error("cache false-hit detected: {0}")]
    FalseHit(String),
    #[error("receipt required for promotion")]
    MissingReceipt,
    #[error("invalid key material: {0}")]
    InvalidKeyMaterial(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}
