//! Error types for SignRail.

use std::fmt::{Display, Formatter};

/// Result alias used by SignRail APIs.
pub type Result<T> = std::result::Result<T, SignRailError>;

/// Release-critical failures.
#[derive(Debug)]
pub enum SignRailError {
    /// Filesystem or process IO failed.
    Io(std::io::Error),
    /// A release policy hard block fired.
    Policy(String),
    /// Signing backend is unavailable and release must fail closed.
    SigningUnavailable(String),
    /// Verification failed.
    Verification(String),
    /// Caller supplied invalid input.
    InvalidInput(String),
}

impl Display for SignRailError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SignRailError::Io(err) => write!(f, "io error: {err}"),
            SignRailError::Policy(msg) => write!(f, "policy block: {msg}"),
            SignRailError::SigningUnavailable(msg) => write!(f, "signing unavailable: {msg}"),
            SignRailError::Verification(msg) => write!(f, "verification failed: {msg}"),
            SignRailError::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for SignRailError {}

impl From<std::io::Error> for SignRailError {
    fn from(value: std::io::Error) -> Self {
        SignRailError::Io(value)
    }
}
