#![doc = "Error and result types for runner crates."]

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Runner-fabric error with a stable machine-readable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerError {
    code: &'static str,
    message: String,
}

impl RunnerError {
    /// Create a new runner error.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Machine-readable error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for RunnerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for RunnerError {}

impl From<std::io::Error> for RunnerError {
    fn from(value: std::io::Error) -> Self {
        Self::new("io", value.to_string())
    }
}

/// Result alias used by the Phase 4 runner fabric.
pub type RunnerResult<T> = Result<T, RunnerError>;
