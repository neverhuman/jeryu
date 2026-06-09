//! Fluent builder for assembling a [`TuiReadModel`] from its sub-snapshots.

use chrono::{DateTime, Utc};

use crate::dashboards::agent_runs::AgentRunsDashboard;
use crate::dashboards::agents::AgentsSnapshot;
use crate::dashboards::approvals::ApprovalsSnapshot;
use crate::dashboards::codegraph::CodegraphDashboard;
use crate::dashboards::evidence::EvidenceSnapshot;
use crate::dashboards::release::ReleaseSnapshot;
use crate::dashboards::runners::RunnersDashboard;
use crate::dashboards::source_doctor::SourceDoctorDashboard;
use crate::dashboards::workcells::WorkcellsDashboard;
use crate::dashboards::workflow::WorkflowSnapshot;
use crate::queue::QueueSnapshot;
use crate::read_model::{
    AttentionItem, MissionSnapshot, NextActionRecommendation, SystemHealth, TuiReadModel,
};
use crate::repos::ReposSnapshot;

/// Fluent builder for a sample read model.
#[derive(Debug, Clone)]
pub struct TuiReadModelBuilder {
    model: TuiReadModel,
}

impl TuiReadModelBuilder {
    /// Start from the crate default (empty, healthy) snapshot.
    pub fn new() -> Self {
        Self {
            model: TuiReadModel::default(),
        }
    }

    /// Pin `generated_at` for deterministic fixtures/snapshots.
    pub fn generated_at(mut self, at: DateTime<Utc>) -> Self {
        self.model.generated_at = at;
        self
    }

    pub fn event_cursor(mut self, cursor: u64) -> Self {
        self.model.event_cursor = cursor;
        self
    }

    pub fn mission(mut self, mission: MissionSnapshot) -> Self {
        self.model.mission = mission;
        self
    }

    pub fn queue(mut self, queue: QueueSnapshot) -> Self {
        self.model.queue = queue;
        self
    }

    pub fn repos(mut self, repos: ReposSnapshot) -> Self {
        self.model.repos = repos;
        self
    }

    pub fn runners(mut self, runners: RunnersDashboard) -> Self {
        self.model.runners = runners;
        self
    }

    pub fn source_doctor(mut self, sd: SourceDoctorDashboard) -> Self {
        self.model.source_doctor = sd;
        self
    }

    pub fn approvals(mut self, approvals: ApprovalsSnapshot) -> Self {
        self.model.approvals = approvals;
        self
    }

    pub fn evidence(mut self, evidence: EvidenceSnapshot) -> Self {
        self.model.evidence = evidence;
        self
    }

    pub fn agents(mut self, agents: AgentsSnapshot) -> Self {
        self.model.agents = agents;
        self
    }

    pub fn agent_runs(mut self, agent_runs: AgentRunsDashboard) -> Self {
        self.model.agent_runs = agent_runs;
        self
    }

    pub fn codegraph(mut self, codegraph: CodegraphDashboard) -> Self {
        self.model.codegraph = codegraph;
        self
    }

    pub fn release(mut self, release: ReleaseSnapshot) -> Self {
        self.model.release = release;
        self
    }

    pub fn workcells(mut self, workcells: WorkcellsDashboard) -> Self {
        self.model.workcells = workcells;
        self
    }

    pub fn workflow(mut self, workflow: WorkflowSnapshot) -> Self {
        self.model.workflow = workflow;
        self
    }

    pub fn attention(mut self, items: Vec<AttentionItem>) -> Self {
        self.model.attention = items;
        self
    }

    pub fn next_action(mut self, action: NextActionRecommendation) -> Self {
        self.model.next_action = Some(action);
        self
    }

    pub fn system(mut self, system: SystemHealth) -> Self {
        self.model.system = system;
        self
    }

    pub fn build(self) -> TuiReadModel {
        self.model
    }
}

impl Default for TuiReadModelBuilder {
    fn default() -> Self {
        Self::new()
    }
}
