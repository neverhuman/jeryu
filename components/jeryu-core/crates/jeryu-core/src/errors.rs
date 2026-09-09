use thiserror::Error;

pub type Result<T> = std::result::Result<T, ForgeError>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ForgeError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    Conflict(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("branch protection blocked the operation: {0}")]
    BranchProtection(String),
    #[error("storage failed: {0}")]
    Storage(String),
}
