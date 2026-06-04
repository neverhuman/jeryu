//! Error types for the code graph crate.

use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, CodeGraphError>;

/// Errors surfaced by the code graph crate.
#[derive(Debug, Error)]
pub enum CodeGraphError {
    /// Underlying SQLite/storage failure.
    #[error("storage error: {0}")]
    Storage(String),

    /// Invalid repo-relative input path.
    #[error("invalid path {path}: {reason}")]
    InvalidPath {
        /// The rejected path.
        path: String,
        /// Human-readable reason.
        reason: String,
    },

    /// Invalid query token budget.
    #[error("invalid max_tokens {value}: {reason}")]
    InvalidMaxTokens {
        /// The rejected value.
        value: u32,
        /// Human-readable reason.
        reason: String,
    },

    /// Workspace graph load failure (from `jeryu-rustjet`).
    #[error("workspace graph error: {0}")]
    Workspace(String),

    /// Filesystem walk/read failure during indexing.
    #[error("indexing error at {path}: {source}")]
    Index {
        /// Path that triggered the failure.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}
