//! Release domain: signed release record plus the [`InMemoryClient`]
//! implementation of the release surface of [`ForgeClient`].

use serde::{Deserialize, Serialize};

use super::{ClientError, ClientResult, InMemoryClient, stable_hash};

/// A signed release record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseRecord {
    /// Release version label.
    pub version: String,
    /// Whether the release gate is satisfied.
    pub ready: bool,
    /// Signed witness digest.
    pub witness: String,
}

// ---------------------------------------------------------------------------
// In-memory implementation: release surface
// ---------------------------------------------------------------------------

impl InMemoryClient {
    pub(super) fn release_ready_inner(&self, version: &str) -> ClientResult<ReleaseRecord> {
        if version.trim().is_empty() {
            return Err(ClientError::Invalid("version is empty".into()));
        }
        Ok(ReleaseRecord {
            version: version.to_string(),
            ready: true,
            witness: stable_hash(version),
        })
    }
}
