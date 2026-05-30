//! Error and result types.

use std::fmt::{Display, Formatter};
use std::io;
use std::path::PathBuf;

/// Result alias used by gitd.
pub type Result<T> = std::result::Result<T, GitdError>;

/// Phase 1 gitd error values.
#[derive(Debug)]
pub enum GitdError {
    /// I/O failure.
    Io(io::Error),
    /// Invalid external input.
    InvalidInput(String),
    /// Path traversal or invalid repository path.
    InvalidPath(String),
    /// Git executable returned a non-zero status.
    GitCommandFailed {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
    /// A requested repository was not found.
    RepoNotFound(PathBuf),
    /// A protected-ref policy denied an update.
    ProtectedRefDenied(String),
    /// Protocol parsing failed.
    Protocol(String),
    /// HTTP parsing or response failure.
    Http(String),
    /// LFS object operation failed.
    Lfs(String),
}

impl Display for GitdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::InvalidPath(msg) => write!(f, "invalid path: {msg}"),
            Self::GitCommandFailed {
                program,
                code,
                stderr,
            } => {
                write!(
                    f,
                    "git command failed: {program} code={code:?} stderr={stderr}"
                )
            }
            Self::RepoNotFound(path) => write!(f, "repository not found: {}", path.display()),
            Self::ProtectedRefDenied(msg) => write!(f, "protected ref denied: {msg}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Http(msg) => write!(f, "http error: {msg}"),
            Self::Lfs(msg) => write!(f, "lfs error: {msg}"),
        }
    }
}

impl std::error::Error for GitdError {}

impl From<io::Error> for GitdError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
