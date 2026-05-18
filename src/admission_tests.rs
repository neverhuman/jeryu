use super::*;

const AGENT_REF: &str = "refs/heads/agent/demo-branch";

fn zero_sha() -> String {
    format!("{:040x}", 0u8)
}

fn head_sha() -> String {
    format!("{:040x}", 81_985_529_216_486_895_u128)
}

#[test]
fn allows_human_or_system_ref() {
    let zero = zero_sha();
    let head = head_sha();
    let eval = evaluate_pre_receive_line(&format!("{zero} {head} refs/heads/main"), false);
    assert_eq!(eval.verdict, AdmissionVerdict::Allow);
    assert_eq!(eval.actor_kind, "human_or_system");
}

#[test]
fn audits_agent_ref_until_ledger_enforcement_is_enabled() {
    let zero = zero_sha();
    let head = head_sha();
    let eval = evaluate_pre_receive_line(&format!("{zero} {head} {AGENT_REF}"), false);
    assert_eq!(eval.verdict, AdmissionVerdict::Audit);
    assert_eq!(eval.actor_kind, "agent");
}

#[test]
fn denies_agent_ref_when_enforcement_is_enabled() {
    let zero = zero_sha();
    let head = head_sha();
    let eval = evaluate_pre_receive_line(&format!("{zero} {head} {AGENT_REF}"), true);
    assert_eq!(eval.verdict, AdmissionVerdict::Deny);
}

#[test]
fn denies_malformed_input() {
    let eval = evaluate_pre_receive_line("not enough fields", false);
    assert_eq!(eval.verdict, AdmissionVerdict::Deny);
}

#[tokio::test]
async fn allows_agent_ref_with_active_ledger_grant() -> anyhow::Result<()> {
    let db = crate::state::Db::open_memory().await?;
    let payload = "{}";
    let intent_id = db
        .record_capability_intent(crate::state::NewCapabilityIntent {
            request_id: "req-admission-test",
            intent_type: "ProposePatch",
            action_id: "propose_patch",
            project_id: Some(1),
            ref_name: Some(AGENT_REF),
            target_ref: Some("main"),
            actor: "capability-api",
            status: "executed",
            payload,
        })
        .await?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    db.approve_capability_grant(crate::state::NewCapabilityGrant {
        intent_id,
        grant_id: "grant-admission-test",
        action_id: "propose_patch",
        project_id: Some(1),
        ref_name: AGENT_REF,
        new_sha: None,
        required_grant: "agent_task",
        status: "approved",
        expires_at: &expires_at,
        payload,
    })
    .await?;

    let eval = evaluate_pre_receive_line_with_db(
        &format!("{} {} {}", zero_sha(), head_sha(), AGENT_REF),
        true,
        &db,
    )
    .await;
    assert_eq!(eval.verdict, AdmissionVerdict::Allow);
    assert_eq!(eval.grant_id.as_deref(), Some("grant-admission-test"));
    assert!(
        eval.reasons
            .contains(&"agent write matched active capability grant".to_string())
    );
    Ok(())
}
