//! Receipts emitted by proof, queue, and agent operations.

use crate::ids::{AgentId, ReceiptId, RepoId};
use std::time::{SystemTime, UNIX_EPOCH};

/// Type of receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptKind {
    /// A proof lane plan was generated.
    ProofPlan,
    /// Proof evidence passed and a witness was minted.
    ProofWitness,
    /// Agent patch was dry-run and scoped.
    AgentDryRunPatch,
    /// Agent fix proposal was accepted by policy.
    AgentProposedFix,
    /// Agent hotfix was accepted by policy.
    AgentHotfix,
    /// A merge queue operation happened.
    MergeQueue,
    /// A repair action was recorded.
    Repair,
}

/// Append-only receipt object. A real deployment would persist and sign this;
/// Phase 7 keeps it deterministic and in-memory for tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    /// Receipt id.
    pub id: ReceiptId,
    /// Receipt kind.
    pub kind: ReceiptKind,
    /// Repository the receipt applies to.
    pub repo: RepoId,
    /// Optional agent identity.
    pub agent: Option<AgentId>,
    /// Subject identifier such as PR id or queue id.
    pub subject: String,
    /// SHA the receipt is bound to.
    pub sha: String,
    /// Human-readable summary.
    pub summary: String,
    /// Commands or typed API calls that produced the receipt.
    pub commands: Vec<String>,
    /// Residual risk statement.
    pub residual_risk: String,
    /// Creation timestamp in milliseconds since epoch.
    pub created_at_ms: u128,
}

impl Receipt {
    /// Creates a new receipt.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: ReceiptKind,
        repo: RepoId,
        agent: Option<AgentId>,
        subject: impl Into<String>,
        sha: impl Into<String>,
        summary: impl Into<String>,
        commands: Vec<String>,
        residual_risk: impl Into<String>,
    ) -> Self {
        // jeryu-core is a domain crate, so reaching for the wall clock here is an
        // IO-in-core smell. It is retained deliberately: `Receipt::new` is public
        // API consumed by several sibling crates (proof engine, runners,
        // ci-scheduler, agentbridge) that all call it with these eight arguments
        // and no timestamp, so threading a `now`/clock parameter through would
        // break those callers — out of scope for this domain-only change.
        let created_at_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_millis(),
            // The system clock is set before the Unix epoch. Receipts are an
            // append-only audit log, so we record 0 rather than fail receipt
            // creation; the anomalous timestamp makes the misconfiguration
            // visible instead of being silently absorbed mid-expression.
            Err(_clock_before_epoch) => 0,
        };
        Self {
            id: ReceiptId::fresh(),
            kind,
            repo,
            agent,
            subject: subject.into(),
            sha: sha.into(),
            summary: summary.into(),
            commands,
            residual_risk: residual_risk.into(),
            created_at_ms,
        }
    }
}
