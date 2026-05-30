//! Error types for CrateVault.

use std::fmt::{Display, Formatter};

/// CrateVault result alias.
pub type Result<T> = std::result::Result<T, VaultError>;

/// Errors emitted by the cache/CAS layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    /// Filesystem or process IO failed.
    Io(String),
    /// Caller supplied invalid input.
    InvalidInput(String),
    /// Policy denied an unsafe action.
    PolicyDenied { law: String, reason: String },
    /// A false hit or object mismatch was detected.
    CachePoisoned(String),
    /// Backend is unavailable.
    StoreUnavailable,
    /// Requested item was not found.
    NotFound(String),
}

impl Display for VaultError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::Io(message) => write!(f, "io error: {message}"),
            VaultError::InvalidInput(message) => write!(f, "invalid input: {message}"),
            VaultError::PolicyDenied { law, reason } => {
                write!(f, "policy denied by {law}: {reason}")
            }
            VaultError::CachePoisoned(message) => write!(f, "cache poisoned: {message}"),
            VaultError::StoreUnavailable => write!(f, "cache store unavailable"),
            VaultError::NotFound(message) => write!(f, "not found: {message}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<std::io::Error> for VaultError {
    fn from(value: std::io::Error) -> Self {
        VaultError::Io(value.to_string())
    }
}
