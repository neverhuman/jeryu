use super::{build_agent_task, compute_slug, format_bot_name};
use crate::gitlab_client::{Issue, ProjectPatResp};

fn fake_issue(iid: i64) -> Issue {
    serde_json::from_value(serde_json::json!({
        "id": iid * 1000,
        "iid": iid,
        "title": "test",
        "state": "opened",
        "labels": ["agent:pending"],
        "web_url": "https://example.com/issues/1",
    }))
    .expect("issue fixture")
}

fn fake_bot() -> ProjectPatResp {
    serde_json::from_value(serde_json::json!({
        "id": 7,
        "name": "@agent-fix-foo-0000",
        "token": "secret",
        "user_id": 42,
    }))
    .expect("bot fixture")
}

#[test]
fn build_agent_task_threads_identity_through_unchanged() {
    // Smoke test: helper preserves the AgentTask shape (issue iid, bot
    // user_id, branch, target_branch) that spawn_agent and spawn_race
    // both depend on. Guards the deduplication boundary.
    let issue = fake_issue(123);
    let bot = fake_bot();
    let task = build_agent_task(
        99,
        "repair foo",
        "agent/repair-foo-x".to_string(),
        "main",
        &issue,
        bot,
    );
    assert_eq!(task.project_id, 99);
    assert_eq!(task.task_description, "repair foo");
    assert_eq!(task.branch_name, "agent/repair-foo-x");
    assert_eq!(task.target_branch, "main");
    assert_eq!(task.issue_iid, Some(123));
    assert_eq!(task.bot_user_id, Some(42));
    assert_eq!(task.bot_token.as_deref(), Some("secret"));
}

#[test]
fn slug_strips_punctuation_lowercases_and_caps_at_four_words() {
    // Punctuation dropped, words joined with '-', lowercased, max 4 words.
    let slug = compute_slug("Fix the BROKEN build, please ASAP!");
    assert_eq!(slug, "fix-the-broken-build");
}

#[test]
fn slug_handles_empty_and_punct_only_input() {
    assert_eq!(compute_slug(""), "");
    assert_eq!(compute_slug("!!!---???"), "");
}

#[test]
fn bot_name_uses_reversed_last_four_timestamp_chars() {
    // Suffix is the timestamp reversed, first 4 chars of the reverse.
    // Timestamp "20260506-120000" -> reverse "000021-60506202" -> "0000".
    let name = format_bot_name("repair-foo", "20260506-120000");
    assert_eq!(name, "@agent-repair-foo-0000");

    // Distinct timestamps with distinct tails produce distinct suffixes.
    let other = format_bot_name("repair-foo", "20260506-120123");
    assert_eq!(other, "@agent-repair-foo-3210");
}
