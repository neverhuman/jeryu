//! Owner: Interactive TUI subsystem — workflow snapshot composite types (U19 first-cut).
//! Proof: `cargo nextest run -p jeryu --lib tui::workflow::model::`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshot::{CacheVerdict, VtiStatus};

use super::edge::WorkflowEdge;
use super::node_kind::WorkflowNodeKind;
use super::phase::WorkflowPhase;
use super::status::WorkflowStatus;

/// A single node in the workflow DAG — one test, check, or gate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub label: String,
    pub command: Option<String>,
    pub kind: WorkflowNodeKind,
    pub status: WorkflowStatus,
    pub required: bool,
    pub critical_path: bool,
    pub deps: Vec<String>,
    pub duration_secs: Option<f64>,
    pub eta_secs: Option<u64>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub backend: Option<WorkflowBackendRef>,
    pub reason: Option<String>,
    pub vti_status: Option<VtiStatus>,
    pub cache_verdict: Option<CacheVerdict>,
    pub progress_pct: Option<u16>,
    pub tags: Vec<String>,
    /// Populated only when `kind == AgentReview { .. }`. Projection of the
    /// reviewer's signed approval receipt for display in the Inspector's
    /// `Agent` sub-tab.
    #[serde(default)]
    pub agent_call: Option<AgentCallDetail>,
}

/// TUI projection of an agent reviewer's call. Sourced from
/// `crate::autonomy::types::AgentApprovalReceipt` by the sync layer; not
/// mutated by the TUI itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCallDetail {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub agent_id: Option<String>,
    /// `"pass"` / `"block"` / `"concern"` / etc.
    pub decision: Option<String>,
    pub reason: Option<String>,
    pub raw_response_sha: Option<String>,
    pub findings: Vec<AgentFindingBrief>,
    /// Truncated raw decision JSON. Useful for debugging when the reviewer
    /// produces structured output that can't be summarized in `reason`.
    pub decision_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentFindingBrief {
    pub severity: String,
    pub class: String,
    pub file: Option<String>,
}

impl From<&crate::autonomy::types::AgentApprovalReceipt> for AgentCallDetail {
    fn from(r: &crate::autonomy::types::AgentApprovalReceipt) -> Self {
        use crate::autonomy::types::{ReviewDecision, Severity};
        let decision = Some(match r.decision {
            ReviewDecision::Pass => "pass".to_string(),
            ReviewDecision::Concern => "concern".to_string(),
            ReviewDecision::Block => "block".to_string(),
            ReviewDecision::Abstain => "abstain".to_string(),
        });
        let findings = r
            .findings
            .iter()
            .map(|f| AgentFindingBrief {
                severity: match f.severity {
                    Severity::Info => "info".into(),
                    Severity::Low => "low".into(),
                    Severity::Medium => "medium".into(),
                    Severity::High => "high".into(),
                    Severity::Critical => "critical".into(),
                },
                class: f.class.clone(),
                file: Some(f.file.clone()),
            })
            .collect();
        AgentCallDetail {
            model: r.model.clone(),
            provider: r.provider.clone(),
            agent_id: Some(r.agent_id.clone()),
            decision,
            reason: r.reason.clone(),
            raw_response_sha: r.raw_response_sha.clone(),
            findings,
            decision_json: None,
        }
    }
}

/// Where a node's live status comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowBackendRef {
    GitlabJob {
        project_id: i64,
        pipeline_id: i64,
        job_id: i64,
    },
    VtiPlanItem {
        plan_id: i64,
        test_id: String,
    },
    LocalProofLane {
        lane: String,
    },
}

/// Aggregate counts for the workflow summary banner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub total: u32,
    pub passed: u32,
    pub running: u32,
    pub waiting: u32,
    pub error: u32,
    pub skipped: u32,
    pub cached: u32,
    pub blocked: u32,
    pub overall_pct: f64,
    pub eta_secs: Option<u64>,
}

impl WorkflowSummary {
    /// Build summary from node statuses.
    pub fn from_nodes(nodes: &[WorkflowNode]) -> Self {
        let mut s = Self {
            total: nodes.len() as u32,
            ..Default::default()
        };
        for n in nodes {
            match n.status {
                WorkflowStatus::Ran => s.passed += 1,
                WorkflowStatus::Running => s.running += 1,
                WorkflowStatus::Waiting => s.waiting += 1,
                WorkflowStatus::Error => s.error += 1,
                WorkflowStatus::Skipped => s.skipped += 1,
                WorkflowStatus::Cached => s.cached += 1,
                WorkflowStatus::Blocked => s.blocked += 1,
                WorkflowStatus::Unknown => {}
            }
        }
        let terminal = s.passed + s.error + s.skipped + s.cached;
        s.overall_pct = if s.total > 0 {
            (terminal as f64 / s.total as f64) * 100.0
        } else {
            0.0
        };
        s
    }
}

/// Where the workflow data came from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSource {
    LatestDbPlan,
    CurrentDiff,
    LivePipeline,
    #[default]
    Demo,
}

/// The complete workflow DAG snapshot consumed by the widget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub generated_at: DateTime<Utc>,
    pub title: String,
    pub source: WorkflowSource,
    pub mode: String,
    pub confidence: f64,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub phases: Vec<WorkflowPhase>,
    pub summary: WorkflowSummary,
    pub selected_node_id: Option<String>,
    pub outdated: bool,
}

impl WorkflowSnapshot {
    /// Create an empty snapshot with no active workflow data.
    pub fn empty() -> Self {
        Self {
            generated_at: Utc::now(),
            title: "No active workflow".into(),
            source: WorkflowSource::Demo,
            mode: "none".into(),
            confidence: 0.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            phases: Vec::new(),
            summary: WorkflowSummary::default(),
            selected_node_id: None,
            outdated: false,
        }
    }

    /// Look up a node by ID.
    pub fn node(&self, id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find nodes in a specific phase.
    pub fn phase_nodes(&self, phase_idx: usize) -> Vec<&WorkflowNode> {
        match self.phases.get(phase_idx) {
            Some(p) => p.node_ids.iter().filter_map(|id| self.node(id)).collect(),
            None => Vec::new(),
        }
    }

    /// Locate the (phase_idx, node_idx) coordinates of a node id.
    /// Used to restore selection after a snapshot rebuild.
    pub fn locate_node(&self, id: &str) -> Option<(usize, usize)> {
        for (pi, phase) in self.phases.iter().enumerate() {
            if let Some(ni) = phase.node_ids.iter().position(|n| n == id) {
                return Some((pi, ni));
            }
        }
        None
    }
}
