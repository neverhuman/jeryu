use mirrorvault::{
    archive_from_github_value, compare_archives, plan_git_mirror, MirrorMode, MirrorSpec,
};
use serde_json::json;

#[test]
fn drift_detects_missing_repository_and_count_delta() {
    let left = archive_from_github_value(json!({
      "repositories": [
        {"owner": {"login": "acme"}, "name": "rocket", "issues": [{"number": 1, "title": "bug", "state": "open"}]},
        {"owner": {"login": "acme"}, "name": "api"}
      ]
    })).unwrap();
    let right = archive_from_github_value(json!({
      "repositories": [
        {"owner": {"login": "acme"}, "name": "rocket", "issues": []}
      ]
    }))
    .unwrap();

    let drift = compare_archives(&left, &right);
    assert!(drift.drift_detected);
    assert_eq!(drift.missing_in_target, vec!["acme/api"]);
    assert_eq!(drift.repository_drifts[0].issue_delta, -1);
}

#[test]
fn mirror_plan_is_argument_vector_not_shell_string() {
    let spec = MirrorSpec {
        name: "rocket".into(),
        source_url: "git@example.com:acme/rocket.git".into(),
        destination_url: "ssh://jitforge/acme/rocket.git".into(),
        local_path: "/tmp/rocket.git".into(),
        mode: MirrorMode::FullSync,
        prune: true,
        allowed_refs: vec!["refs/heads/*".into(), "refs/tags/*".into()],
    };
    let plan = plan_git_mirror(&spec);
    assert_eq!(plan.commands[0][0], "git");
    assert!(plan
        .commands
        .iter()
        .flatten()
        .any(|part| part.contains("jitforge-allowed-refs")));
}
