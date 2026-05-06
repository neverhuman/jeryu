use super::*;

#[path = "capability_execute.rs"]
mod execute;
#[path = "capability_inspect.rs"]
mod inspect;
#[path = "capability_request.rs"]
mod request;

pub(crate) async fn execute_intent(
    intent: AgentIntent,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match intent {
        AgentIntent::FetchCapsule { job_id } => execute::fetch_capsule(job_id).await,
        AgentIntent::RunTests {
            project_id,
            target_ref,
            test_scope,
        } => execute::run_tests(project_id, target_ref, test_scope, ctx, client).await,
        AgentIntent::ProposePatch {
            project_id,
            branch_name,
            base_ref,
            commit_message,
            modifications,
            mr_title,
        } => execute::propose_patch(
            project_id,
            branch_name,
            base_ref,
            commit_message,
            modifications,
            mr_title,
            ctx,
            client,
        )
        .await,
        AgentIntent::RacePatches {
            project_id,
            base_branch,
            commit_message,
            hypotheses,
        } => {
            execute::race_patches(project_id, base_branch, commit_message, hypotheses, ctx, client)
                .await
        }
        AgentIntent::RequestMerge {
            project_id,
            mr_iid,
            source_branch,
            target_branch,
        } => execute::request_merge(project_id, mr_iid, source_branch, target_branch, client).await,
        AgentIntent::ExplainBlockers {
            entity_type,
            entity_id,
        } => inspect::explain_blockers(entity_type, entity_id, client).await,
        AgentIntent::GetSystemSnapshot => inspect::get_system_snapshot(client).await,
        AgentIntent::GetPipelineJobs {
            project_id,
            pipeline_id,
        } => inspect::get_pipeline_jobs(project_id, pipeline_id, client).await,
        AgentIntent::GetCiBottlenecks {
            project_id,
            ref_name,
            limit,
        } => inspect::get_ci_bottlenecks(project_id, ref_name, limit, client).await,
        AgentIntent::ListAllowedActions => inspect::list_allowed_actions(),
        AgentIntent::PlanValidation {
            project_id,
            ref_name,
            test_ids,
        } => inspect::plan_validation(project_id, ref_name, test_ids).await,
    }
}

pub(crate) fn parse_capability_request(bytes: &[u8]) -> anyhow::Result<ParsedCapabilityRequest> {
    request::parse_capability_request(bytes)
}

pub(crate) fn validate_capability_request(
    parsed: ParsedCapabilityRequest,
) -> anyhow::Result<(AgentIntent, CapabilityContext)> {
    request::validate_capability_request(parsed)
}
