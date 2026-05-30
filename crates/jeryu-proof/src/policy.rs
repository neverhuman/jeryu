//! Jankurai maps and policy types.

use crate::matcher::PathPattern;
use jeryu_core::phase7::{ChangedPath, PullRequestId, RepoId};

/// Owner rule from owner-map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerRule {
    /// Matching pattern.
    pub pattern: PathPattern,
    /// Owners for matched paths.
    pub owners: Vec<String>,
}

/// Test/proof lane rule from test-map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestRule {
    /// Matching pattern.
    pub pattern: PathPattern,
    /// Lanes required for matched paths.
    pub lanes: Vec<String>,
}

/// Generated zone rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedZone {
    /// Matching pattern.
    pub pattern: PathPattern,
    /// Whether agent edits are allowed.
    pub allow_agent_edits: bool,
    /// Why the zone exists.
    pub reason: String,
}

/// Proof lane definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofLane {
    /// Lane name.
    pub name: String,
    /// Required validation commands.
    pub commands: Vec<String>,
    /// Whether the lane is required.
    pub required: bool,
}

/// Change set entering the proof engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeSet {
    /// Repository id.
    pub repo: RepoId,
    /// PR id.
    pub pr: PullRequestId,
    /// Head SHA.
    pub head_sha: String,
    /// Changed paths.
    pub paths: Vec<ChangedPath>,
    /// Whether the actor is an agent.
    pub agent_authored: bool,
}

/// Generated proof plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofPlan {
    /// Repository id.
    pub repo: RepoId,
    /// PR id.
    pub pr: PullRequestId,
    /// Head SHA.
    pub head_sha: String,
    /// Covered paths.
    pub paths: Vec<String>,
    /// Owners resolved for the paths.
    pub owners: Vec<String>,
    /// Required lanes.
    pub lanes: Vec<ProofLane>,
}

/// Evidence for a proof lane execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofEvidence {
    /// Lane name.
    pub lane: String,
    /// Commands executed.
    pub commands: Vec<String>,
    /// Whether the lane passed.
    pub success: bool,
    /// Log digest or deterministic transcript hash text.
    pub log_digest: String,
}

/// Blocker returned by proof planning or verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProofBlocker {
    /// No owner rule covered the path.
    OwnerlessPath(String),
    /// No test/proof lane covered the path.
    UnmappedProofLane(String),
    /// Direct edit to a generated zone was denied.
    GeneratedZoneEditDenied { path: String, reason: String },
    /// Required lane evidence missing.
    MissingEvidence(String),
    /// Lane failed.
    FailedLane(String),
}

impl ProofBlocker {
    /// Human-readable blocker message.
    pub fn message(&self) -> String {
        match self {
            Self::OwnerlessPath(path) => format!("ownerless path blocks merge: {path}"),
            Self::UnmappedProofLane(path) => format!("unmapped proof lane blocks merge: {path}"),
            Self::GeneratedZoneEditDenied { path, reason } => {
                format!("generated zone edit denied for {path}: {reason}")
            }
            Self::MissingEvidence(lane) => format!("missing proof evidence for lane: {lane}"),
            Self::FailedLane(lane) => format!("proof lane failed: {lane}"),
        }
    }
}
