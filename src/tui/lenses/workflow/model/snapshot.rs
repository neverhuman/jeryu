//! Workflow graph and snapshot model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::snapshot::{CacheVerdict, VtiStatus};

use super::{node_kind::WorkflowNodeKind, status::WorkflowStatus};

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
    #[serde(default)]
    pub agent_call: Option<AgentCallDetail>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCallDetail {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub agent_id: Option<String>,
    pub decision: Option<String>,
    pub reason: Option<String>,
    pub raw_response_sha: Option<String>,
    pub findings: Vec<AgentFindingBrief>,
    pub decision_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentFindingBrief {
    pub severity: String,
    pub class: String,
    pub file: Option<String>,
}

impl From<&crate::autonomy::types::AgentApprovalReceipt> for AgentCallDetail {
    fn from(receipt: &crate::autonomy::types::AgentApprovalReceipt) -> Self {
        use crate::autonomy::types::{ReviewDecision, Severity};

        let decision = Some(match receipt.decision {
            ReviewDecision::Pass => "pass".to_string(),
            ReviewDecision::Concern => "concern".to_string(),
            ReviewDecision::Block => "block".to_string(),
            ReviewDecision::Abstain => "abstain".to_string(),
        });
        let findings = receipt
            .findings
            .iter()
            .map(|finding| AgentFindingBrief {
                severity: match finding.severity {
                    Severity::Info => "info".into(),
                    Severity::Low => "low".into(),
                    Severity::Medium => "medium".into(),
                    Severity::High => "high".into(),
                    Severity::Critical => "critical".into(),
                },
                class: finding.class.clone(),
                file: Some(finding.file.clone()),
            })
            .collect();

        AgentCallDetail {
            model: receipt.model.clone(),
            provider: receipt.provider.clone(),
            agent_id: Some(receipt.agent_id.clone()),
            decision,
            reason: receipt.reason.clone(),
            raw_response_sha: receipt.raw_response_sha.clone(),
            findings,
            decision_json: None,
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    pub kind: WorkflowEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEdgeKind {
    Dependency,
    StageOrder,
    VtiSkip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPhase {
    pub id: String,
    pub title: String,
    pub depth: u32,
    pub node_ids: Vec<String>,
}

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
    pub fn from_nodes(nodes: &[WorkflowNode]) -> Self {
        let mut summary = Self {
            total: nodes.len() as u32,
            ..Default::default()
        };
        for node in nodes {
            match node.status {
                WorkflowStatus::Ran => summary.passed += 1,
                WorkflowStatus::Running => summary.running += 1,
                WorkflowStatus::Waiting => summary.waiting += 1,
                WorkflowStatus::Error => summary.error += 1,
                WorkflowStatus::Skipped => summary.skipped += 1,
                WorkflowStatus::Cached => summary.cached += 1,
                WorkflowStatus::Blocked => summary.blocked += 1,
                WorkflowStatus::Unknown => {}
            }
        }
        let terminal = summary.passed + summary.error + summary.skipped + summary.cached;
        summary.overall_pct = if summary.total > 0 {
            (terminal as f64 / summary.total as f64) * 100.0
        } else {
            0.0
        };
        summary
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSource {
    LatestDbPlan,
    CurrentDiff,
    LivePipeline,
    #[default]
    Demo,
}

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

    pub fn node(&self, id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn phase_nodes(&self, phase_idx: usize) -> Vec<&WorkflowNode> {
        match self.phases.get(phase_idx) {
            Some(phase) => phase
                .node_ids
                .iter()
                .filter_map(|id| self.node(id))
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn locate_node(&self, id: &str) -> Option<(usize, usize)> {
        for (phase_idx, phase) in self.phases.iter().enumerate() {
            if let Some(node_idx) = phase.node_ids.iter().position(|node| node == id) {
                return Some((phase_idx, node_idx));
            }
        }
        None
    }
}
