//! Error model for Phase 7.

use std::fmt::{Display, Formatter};

/// Convenient result alias for JitForge operations.
pub type JitForgeResult<T> = Result<T, JitForgeError>;

/// Policy-aware errors. These are intentionally explicit so failures are
/// repairable by humans and agents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitForgeError {
    /// A requested entity was not found.
    NotFound(String),
    /// Input was malformed or semantically invalid.
    Invalid(String),
    /// A Jankurai or queue policy denied the operation.
    PolicyDenied(String),
    /// The merge queue detected a conflict.
    Conflict(String),
    /// A required receipt was missing.
    MissingReceipt(String),
    /// A proof witness was required but absent or invalid.
    MissingProofWitness(String),
}

impl Display for JitForgeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid: {msg}"),
            Self::PolicyDenied(msg) => write!(f, "policy denied: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::MissingReceipt(msg) => write!(f, "missing receipt: {msg}"),
            Self::MissingProofWitness(msg) => write!(f, "missing proof witness: {msg}"),
        }
    }
}

impl std::error::Error for JitForgeError {}
