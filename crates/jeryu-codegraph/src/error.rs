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

    /// Workspace graph load failure (from `jeryu-rustjet`).
    #[error("workspace graph error: {0}")]
    Workspace(String),

    /// Governance metadata load or parse failure.
    #[error("governance metadata error at {path}: {message}")]
    Governance {
        /// Governance file that triggered the failure.
        path: String,
        /// Parse or load message.
        message: String,
    },

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
