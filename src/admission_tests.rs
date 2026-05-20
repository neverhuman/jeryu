use super::*;

const AGENT_REF: &str = "refs/heads/agent/demo-branch";

fn zero_sha() -> String {
    format!("{:040x}", 0u8)
}

fn head_sha() -> String {
    format!("{:040x}", 81_985_529_216_486_895_u128)
}

fn next_sha() -> String {
    format!("{:040x}", 81_985_529_216_486_896_u128)
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
fn denies_direct_update_to_protected_main() {
    let eval = evaluate_pre_receive_line_with_actor(
        &format!("{} {} refs/heads/main", head_sha(), next_sha()),
        false,
        None,
    );
    assert_eq!(eval.verdict, AdmissionVerdict::Deny);
    assert!(
        eval.reasons
            .contains(&"protected branch direct push denied; use JeRyu main-relay".to_string())
    );
}

#[test]
fn denies_delete_of_protected_main() {
    let eval = evaluate_pre_receive_line_with_actor(
        &format!("{} {} refs/heads/main", head_sha(), zero_sha()),
        false,
        None,
    );
    assert_eq!(eval.verdict, AdmissionVerdict::Deny);
    assert!(
        eval.reasons
            .contains(&"protected branch removal denied".to_string())
    );
}

#[test]
fn denies_protected_tag_rewrite_and_delete() {
    let rewrite = evaluate_pre_receive_line(
        &format!("{} {} refs/tags/v1.0.0", head_sha(), next_sha()),
        false,
    );
    assert_eq!(rewrite.verdict, AdmissionVerdict::Deny);
    assert!(
        rewrite
            .reasons
            .contains(&"protected tag rewrite denied".to_string())
    );

    let removal = evaluate_pre_receive_line(
        &format!("{} {} refs/tags/v1.0.0", head_sha(), zero_sha()),
        false,
    );
    assert_eq!(removal.verdict, AdmissionVerdict::Deny);
    assert!(
        removal
            .reasons
            .contains(&"protected tag removal denied".to_string())
    );
}

#[test]
fn denies_non_fast_forward_protected_branch_marker() {
    let eval = evaluate_pre_receive_line_with_context(
        &format!("{} {} refs/heads/main", head_sha(), next_sha()),
        false,
        None,
        true,
    );
    assert_eq!(eval.verdict, AdmissionVerdict::Deny);
    assert!(
        eval.reasons
            .contains(&"non-fast-forward protected branch push denied".to_string())
    );
}

#[test]
fn allows_jeryu_main_relay_actor_to_update_main() {
    let eval = evaluate_pre_receive_line_with_actor(
        &format!("{} {} refs/heads/main", head_sha(), next_sha()),
        false,
        Some("jeryu"),
    );
    assert_eq!(eval.verdict, AdmissionVerdict::Allow);
    assert_eq!(eval.actor_kind, "jeryu");
}

#[test]
fn audits_agent_ref_until_grant_enforcement_is_enabled() {
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
async fn allows_agent_ref_with_active_grant() -> anyhow::Result<()> {
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

    let eval = crate::db::admission_repo::evaluate_line_with_records(
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
