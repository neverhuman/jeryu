use super::*;

#[path = "capability_inspect_read.rs"]
mod read;
#[path = "capability_inspect_snapshot.rs"]
mod snapshot;
pub(crate) use read::{explain_blockers, get_ci_bottlenecks, plan_validation};
pub(crate) use snapshot::{get_pipeline_jobs, get_system_snapshot};

pub(crate) fn list_allowed_actions() -> CapabilityResponse {
    CapabilityResponse {
        success: true,
        message: "allowed actions".into(),
        data: Some(serde_json::json!({
            "actions": ["FetchCapsule", "RunTests", "ProposePatch", "RacePatches", "RequestMerge", "ExplainBlockers", "GetSystemSnapshot", "GetPipelineJobs", "GetCiBottlenecks", "ListAllowedActions", "PlanValidation"]
        })),
    }
}

fn err(msg: &str) -> CapabilityResponse {
    CapabilityResponse::error(msg)
}
