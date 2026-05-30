//! Rollback metadata required for releases.

use crate::error::{Result, SignRailError};
use crate::json;

/// Rollback metadata attached to a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollbackMetadata {
    /// Previous release identifier.
    pub previous_release: String,
    /// Operator-safe rollback command.
    pub rollback_command: String,
    /// Configuration digest expected by rollback.
    pub config_digest: String,
    /// Data migration note or identifier.
    pub data_migration: String,
    /// Verification time as Unix epoch seconds.
    pub verified_at_epoch: u64,
}

impl RollbackMetadata {
    /// Construct rollback metadata.
    pub fn new(
        previous_release: impl Into<String>,
        rollback_command: impl Into<String>,
        config_digest: impl Into<String>,
        data_migration: impl Into<String>,
        verified_at_epoch: u64,
    ) -> Self {
        Self {
            previous_release: previous_release.into(),
            rollback_command: rollback_command.into(),
            config_digest: config_digest.into(),
            data_migration: data_migration.into(),
            verified_at_epoch,
        }
    }

    /// Validate rollback metadata is actionable.
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("previous_release", &self.previous_release),
            ("rollback_command", &self.rollback_command),
            ("config_digest", &self.config_digest),
            ("data_migration", &self.data_migration),
        ] {
            if value.trim().is_empty() {
                return Err(SignRailError::Policy(format!(
                    "rollback metadata missing {name}"
                )));
            }
        }
        Ok(())
    }

    /// Render rollback metadata JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{}}}",
            json::field("previous_release", &self.previous_release),
            json::field("rollback_command", &self.rollback_command),
            json::field("config_digest", &self.config_digest),
            json::field("data_migration", &self.data_migration),
            json::number_field("verified_at_epoch", self.verified_at_epoch)
        )
    }
}
