//! Owner: Interactive TUI subsystem — pipeline graph rendering
//! Proof: `cargo nextest run -p jeryu -- tui::graph`
//! Invariants: Graph rendering is deterministic and never mutates pipeline or release state.
use crate::state::JobEvent;
use crate::tui::live::is_live_job_status;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    Source,
    Admission,
    Impact,
    Build,
    TestMatrix,
    ReleaseGates,
    Prod,
}

impl Phase {
    pub fn to_str(&self) -> &'static str {
        match self {
            Phase::Source => "Source",
            Phase::Admission => "Admission",
            Phase::Impact => "Impact",
            Phase::Build => "Build",
            Phase::TestMatrix => "Test Matrix",
            Phase::ReleaseGates => "Release Gates",
            Phase::Prod => "Prod",
        }
    }

    pub fn phase_index(&self) -> usize {
        match self {
            Phase::Source => 0,
            Phase::Admission => 1,
            Phase::Impact => 2,
            Phase::Build => 3,
            Phase::TestMatrix => 4,
            Phase::ReleaseGates => 5,
            Phase::Prod => 6,
        }
    }
}

pub fn classify_phase(job_name: &str) -> Phase {
    let lower = job_name.to_lowercase();
    if lower.contains("hook") || lower.contains("policy") || lower.contains("admission") {
        Phase::Admission
    } else if lower.contains("impact") || lower.contains("plan") {
        Phase::Impact
    } else if lower.contains("build") || lower.contains("compile") || lower.contains("image") {
        Phase::Build
    } else if lower.contains("test")
        || lower.contains("unit")
        || lower.contains("integration")
        || lower.contains("e2e")
        || lower.contains("lint")
        || lower.contains("fmt")
    {
        Phase::TestMatrix
    } else if lower.contains("canary")
        || lower.contains("remote")
        || lower.contains("telemetry")
        || lower.contains("gate")
        || lower.contains("publish")
    {
        Phase::ReleaseGates
    } else if lower.contains("prod") || lower.contains("deploy") {
        Phase::Prod
    } else {
        // Recovery default
        Phase::Build
    }
}

pub fn classify_lane(job_name: &str) -> String {
    let lower = job_name.to_lowercase();
    if lower.contains("unit") {
        "unit".to_string()
    } else if lower.contains("integration") {
        "integration".to_string()
    } else if lower.contains("e2e") {
        "e2e".to_string()
    } else if lower.contains("lint") || lower.contains("fmt") {
        "static".to_string()
    } else {
        "default".to_string()
    }
}

#[derive(Debug, Clone)]
pub struct JobNode {
    pub job: JobEvent,
    pub phase: Phase,
    pub lane: String,
    pub active: bool,
    pub elapsed_secs: i64,
}

#[derive(Debug, Clone)]
pub struct PipelineGraph {
    pub pipeline_id: i64,
    pub nodes: Vec<JobNode>,
    // phase -> vec of indices in nodes array
    pub phase_layout: BTreeMap<Phase, Vec<usize>>,
    pub critical_path_job_id: Option<i64>,
    pub blockers: usize,
}

impl PipelineGraph {
    pub fn build(pipeline_id: i64, jobs: Vec<JobEvent>) -> Self {
        let mut nodes = Vec::new();
        let mut phase_layout: BTreeMap<Phase, Vec<usize>> = BTreeMap::new();
        let now = chrono::Utc::now();
        let mut critical_path_job_id = None;
        let mut longest_duration = -1;
        let mut blockers = 0;

        for job in jobs {
            let name = job.job_name.as_deref().unwrap_or("unknown");
            let phase = classify_phase(name);
            let lane = classify_lane(name);

            let active = is_live_job_status(job.status.as_str());

            let mut elapsed_secs = 0;
            if let Ok(st) = chrono::DateTime::parse_from_rfc3339(&job.received_at) {
                elapsed_secs = now.signed_duration_since(st).num_seconds();
            }

            if active && elapsed_secs > longest_duration {
                longest_duration = elapsed_secs;
                critical_path_job_id = Some(job.job_id);
            }

            if job.status == "failed" || job.status == "blocked" {
                blockers += 1;
            }

            let idx = nodes.len();
            nodes.push(JobNode {
                job,
                phase,
                lane,
                active,
                elapsed_secs,
            });

            phase_layout.entry(phase).or_default().push(idx);
        }

        Self {
            pipeline_id,
            nodes,
            phase_layout,
            critical_path_job_id,
            blockers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lane_classification() {
        assert_eq!(classify_lane("test-unit-core"), "unit");
        assert_eq!(classify_lane("integration-store"), "integration");
        assert_eq!(classify_lane("test-e2e-api"), "e2e");
    }

    #[test]
    fn test_phase_classification() {
        assert_eq!(classify_phase("build-enclave-server"), Phase::Build);
        assert_eq!(classify_phase("test-unit-core"), Phase::TestMatrix);
        assert_eq!(classify_phase("risk gate"), Phase::ReleaseGates);
        assert_eq!(classify_phase("canary"), Phase::ReleaseGates);
        assert_eq!(classify_phase("prod-deploy"), Phase::Prod);
        assert_eq!(classify_phase("admission-policy"), Phase::Admission);
        assert_eq!(classify_phase("impact-plan"), Phase::Impact);
    }

    #[test]
    fn test_graph_building() {
        let job1 = JobEvent {
            job_id: 1,
            project_id: 1,
            pipeline_id: Some(10),
            status: "running".to_string(),
            job_name: Some("test-unit-core".to_string()),
            pool_name: None,
            system_id: None,
            queued_duration: Some(10.0),
            received_at: chrono::Utc::now().to_rfc3339(),
        };

        let graph = PipelineGraph::build(10, vec![job1]);
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.phase_layout.contains_key(&Phase::TestMatrix));
        assert_eq!(graph.critical_path_job_id, Some(1));
    }
}
