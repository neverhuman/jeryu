use super::*;
use crate::api::entity::EntityKind;

#[test]
fn actions_for_job_include_requeue_and_logs() {
    let entity = EntityRef::new(EntityKind::Job, "14445");
    let actions = actions_for_entity(&entity);
    let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
    assert!(ids.contains(&"open_logs"), "missing open_logs");
    assert!(ids.contains(&"requeue_job"), "missing requeue_job");
    assert!(
        ids.contains(&"explain_blockers"),
        "missing explain_blockers"
    );
}

#[test]
fn actions_for_pool_include_pause() {
    let entity = EntityRef::new(EntityKind::Pool, "rust-large");
    let actions = actions_for_entity(&entity);
    let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
    assert!(ids.contains(&"pause_pool"), "missing pause_pool");
    assert!(
        !ids.contains(&"requeue_job"),
        "requeue_job should not apply to pool"
    );
}

#[test]
fn actions_for_mr_include_merge_and_tests() {
    let entity = EntityRef::new(EntityKind::MergeRequest, "42");
    let actions = actions_for_entity(&entity);
    let ids: Vec<_> = actions.iter().map(|a| a.action_id).collect();
    assert!(ids.contains(&"request_merge"), "missing request_merge");
    assert!(ids.contains(&"run_tests"), "missing run_tests");
    assert!(ids.contains(&"propose_patch"), "missing propose_patch");
}

#[test]
fn default_preview_is_disabled() {
    let preview = ActionPreview::default();
    assert!(!preview.enabled);
    assert!(preview.disabled_reason.is_some());
}
