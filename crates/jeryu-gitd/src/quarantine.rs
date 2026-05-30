//! Receive-pack quarantine helpers.

use crate::error::{GitdError, Result};
use std::env;
use std::path::PathBuf;

/// Git quarantine environment visible to pre-receive hooks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuarantineEnv {
    /// Temporary object directory used by Git while receiving a push.
    pub object_directory: Option<PathBuf>,
    /// Alternate object directories.
    pub alternate_object_directories: Option<String>,
}

impl QuarantineEnv {
    /// Read quarantine-related environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            object_directory: env::var_os("GIT_OBJECT_DIRECTORY").map(PathBuf::from),
            alternate_object_directories: env::var("GIT_ALTERNATE_OBJECT_DIRECTORIES").ok(),
        }
    }

    /// Require that a hook is running in a quarantine context.
    pub fn require_for_receive(&self) -> Result<()> {
        if self.object_directory.is_none() {
            return Err(GitdError::InvalidInput(
                "pre-receive guard requires Git quarantine env".to_string(),
            ));
        }
        Ok(())
    }
}
