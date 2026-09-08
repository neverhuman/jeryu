//! Proof domain: verdict type plus the [`InMemoryClient`] implementation of
//! the proof surface of [`ForgeClient`].

use serde::{Deserialize, Serialize};

use super::{ClientError, ClientResult, InMemoryClient, stable_hash};

/// A proof verdict for a changeset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofVerdict {
    /// Whether the changeset is admissible.
    pub admissible: bool,
    /// Stable plan hash; identical changesets verify to identical hashes.
    pub plan_hash: String,
    /// Ordered blocker reasons when not admissible.
    pub blockers: Vec<String>,
}

// ---------------------------------------------------------------------------
// In-memory implementation: proof surface
// ---------------------------------------------------------------------------

impl InMemoryClient {
    pub(super) fn proof_verify_inner(&self, changeset: &str) -> ClientResult<ProofVerdict> {
        if changeset.trim().is_empty() {
            return Err(ClientError::Invalid("changeset is empty".into()));
        }
        // Deterministic, content-derived hash: identical changesets verify to
        // an identical plan hash (replay-stable contract).
        let admissible = !changeset.contains("FORBIDDEN");
        let plan_hash = stable_hash(changeset);
        let blockers = if admissible {
            Vec::new()
        } else {
            vec!["changeset touches a forbidden path".to_string()]
        };
        Ok(ProofVerdict {
            admissible,
            plan_hash,
            blockers,
        })
    }

    pub(super) fn proof_explain_inner(&self, id: &str) -> ClientResult<ProofVerdict> {
        if id.trim().is_empty() {
            return Err(ClientError::Invalid("blocker id is empty".into()));
        }
        Ok(ProofVerdict {
            admissible: false,
            plan_hash: stable_hash(id),
            blockers: vec![format!("blocker {id}: gate not satisfied")],
        })
    }
}
