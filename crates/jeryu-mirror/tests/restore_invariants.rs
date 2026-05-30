use jeryu_mirror::{
    InMemoryRestoreTarget, RestoreOptions, RestoreTarget, archive_from_github_value, plan_restore,
};
use serde_json::json;

#[test]
fn restore_plan_counts_objects_and_secrets() {
    let archive = archive_from_github_value(json!({
      "repositories": [{
        "owner": {"login": "acme"},
        "name": "rocket",
        "issues": [{"number": 1, "title": "bug", "state": "open"}],
        "pull_requests": [{"number": 2, "title": "fix", "state": "open"}],
        "hooks": [{"id": 1, "config": {"url": "https://ci.invalid", "secret": "hidden"}}],
        "app_installations": [{"id": 2, "slug": "ci-bot"}]
      }]
    }))
    .unwrap();

    let mut target = InMemoryRestoreTarget::default();
    let report = plan_restore(&archive, &mut target, RestoreOptions::default()).unwrap();
    assert!(report.dry_run);
    assert_eq!(report.repositories_planned, 1);
    assert_eq!(report.issues_planned, 1);
    assert_eq!(report.pull_requests_planned, 1);
    assert_eq!(report.webhooks_planned, 1);
    assert_eq!(report.app_installations_planned, 1);
    assert!(
        report
            .secret_rehydration_required
            .contains(&"jeryu_mirror/webhook/1".to_string())
    );
    assert!(
        report
            .secret_rehydration_required
            .contains(&"jeryu_mirror/app/2/token".to_string())
    );
}

#[test]
fn restore_blocks_existing_repo_when_empty_target_required() {
    let archive = archive_from_github_value(json!({
      "repositories": [{"owner": {"login": "acme"}, "name": "rocket"}]
    }))
    .unwrap();
    let mut target = InMemoryRestoreTarget::default();
    target.create_repository(&archive.repositories[0]).unwrap();

    let report = plan_restore(
        &archive,
        &mut target,
        RestoreOptions {
            require_empty_target: true,
            ..RestoreOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        report.blockers,
        vec!["target already has repository acme/rocket"]
    );
}
