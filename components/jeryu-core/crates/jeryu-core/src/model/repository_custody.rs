//! Persistent reasons a repository cannot admit another mutation.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A restriction recorded by the trusted control plane. The evidence reference
/// is custody data; it does not itself authenticate a reviewer or operator.
/// No ordinary mutation API can clear or replace a recorded restriction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepositoryMutationBlock {
    ReadOnly { reason: String, evidence: String },
    ReconciliationRequired { operation_id: Uuid, reason: String },
}
