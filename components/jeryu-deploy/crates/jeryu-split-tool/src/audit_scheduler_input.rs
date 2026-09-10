//! Validate planner data without authenticating its Git source or event.
use super::{Plan, Request, attempt_key, digest, hex, source_key, validate};
use anyhow::{Context, Result, ensure};

/// Validate the planner's wire contract and key consistency, not Git/event authenticity.
pub(crate) fn import_plan(bytes: &[u8]) -> Result<Plan> {
    let shape: serde_json::Value = serde_json::from_slice(bytes)?;
    ensure!(
        shape.is_object() && shape["identity"].is_object(),
        "audit plan objects required"
    );
    ensure!(
        shape["jobs"]
            .as_array()
            .is_some_and(|jobs| jobs.iter().all(serde_json::Value::is_object)),
        "audit job objects required"
    );
    let plan: Plan = serde_json::from_slice(bytes).context("invalid audit plan")?;
    ensure!(
        plan.schema_version == "jeryu.audit-plan/v1",
        "unsupported audit plan"
    );
    ensure!(
        !plan.execution_verified && !plan.publication_qualified,
        "plan cannot claim execution or publication authority"
    );
    validate(&Request {
        schema_version: "jeryu.audit-plan-request/v1".into(),
        identity: plan.identity.clone(),
        event: plan.event,
        source_ref: plan.source_ref.clone(),
        before: plan.before.clone(),
        after: plan.after.clone(),
        run_id: plan.run_id.clone(),
        run_attempt: plan.run_attempt,
        previous_run_id: plan.previous_run_id.clone(),
    })?;
    ensure!(
        plan.resolved_after_commit
            .as_ref()
            .is_none_or(|id| hex(id, 40)),
        "invalid resolved commit"
    );
    ensure!(
        plan.withdrawn_commits.iter().all(|id| hex(id, 40)),
        "invalid withdrawn commit"
    );
    ensure!(
        matches!(
            plan.disposition.as_str(),
            "excluded_evidence_branch"
                | "exact_revision"
                | "rewritten"
                | "unchanged"
                | "advanced"
                | "reconciled"
                | "created"
                | "deleted"
        ),
        "invalid plan disposition"
    );
    ensure!(
        (plan.disposition == "excluded_evidence_branch")
            == (plan.source_ref == "refs/heads/audit-evidence"),
        "evidence disposition does not match branch"
    );
    if plan.source_ref == "refs/heads/audit-evidence" {
        ensure!(
            plan.disposition == "excluded_evidence_branch" && plan.jobs.is_empty(),
            "generated evidence branch cannot enqueue source"
        );
    }
    if matches!(
        plan.disposition.as_str(),
        "excluded_evidence_branch" | "unchanged" | "deleted"
    ) {
        ensure!(plan.jobs.is_empty(), "non-source disposition contains jobs");
    }
    if plan.disposition == "rewritten" {
        ensure!(
            plan.resolved_after_commit.is_some() && !plan.withdrawn_commits.is_empty(),
            "rewritten history needs explicit withdrawn commits"
        );
    } else if !matches!(
        plan.disposition.as_str(),
        "excluded_evidence_branch" | "unchanged" | "deleted"
    ) {
        ensure!(
            !plan.jobs.is_empty() && plan.resolved_after_commit.is_some(),
            "source disposition needs explicit jobs"
        );
    }
    let mut keys = std::collections::BTreeSet::new();
    for job in &plan.jobs {
        ensure!(
            hex(&job.source_commit, 40)
                && hex(&job.source_tree, 40)
                && job.source_commit != "0".repeat(40)
                && job.source_tree != "0".repeat(40),
            "invalid planned source identity"
        );
        ensure!(job.status == "pending", "plan job cannot claim a result");
        ensure!(
            job.deduplication_key
                == source_key(&plan.identity, &job.source_commit, &job.source_tree)?,
            "incorrect source deduplication key"
        );
        ensure!(
            job.attempt_key == attempt_key(&job.deduplication_key, &plan.run_id, plan.run_attempt)?,
            "incorrect scheduled attempt key"
        );
        ensure!(
            keys.insert(&job.deduplication_key),
            "duplicate source job in plan"
        );
    }
    Ok(plan)
}

pub(crate) fn plan_event_key(plan: &Plan) -> Result<String> {
    digest(&(
        "jeryu.audit-plan-event/v1",
        &plan.identity,
        plan.event,
        &plan.source_ref,
        &plan.before,
        &plan.after,
        &plan.run_id,
        plan.run_attempt,
    ))
}
