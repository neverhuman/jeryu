use jeryu_mirror::archive_from_github_value;
use serde_json::json;

#[test]
fn github_backup_imports_phase9_metadata() {
    let archive = archive_from_github_value(json!({
      "source": "github.com/acme",
      "repositories": [{
        "owner": {"login": "acme"},
        "name": "rocket",
        "private": true,
        "default_branch": "main",
        "issues": [{"number": 1, "title": "bug", "state": "open", "labels": [{"name": "bug"}]}],
        "pull_requests": [{"number": 2, "title": "fix", "state": "closed", "merged_at": "2026-01-01T00:00:00Z"}],
        "releases": [{"tag_name": "v1.0.0", "assets": [{"name": "jeryu.tar.gz", "size": 42, "sha256": "sha256:abc"}]}],
        "artifacts": [{"name": "linux-amd64", "size_in_bytes": 9, "expired": false}],
        "hooks": [{"id": 7, "config": {"url": "https://ci.invalid/hook", "secret": "hidden"}, "events": ["push", "pull_request"]}],
        "app_installations": [{"id": 9, "slug": "ci-bot", "permissions": {"checks": "write"}}],
        "protected_branches": [{"name": "main", "required_status_checks": {"contexts": ["ci/full"]}, "required_pull_request_reviews": {"required_approving_review_count": 2}}]
      }]
    }))
    .expect("github archive");

    let repo = &archive.repositories[0];
    assert_eq!(repo.full_name(), "acme/rocket");
    assert_eq!(repo.issues.len(), 1);
    assert_eq!(repo.pull_requests.len(), 1);
    assert_eq!(repo.releases[0].assets.len(), 1);
    assert_eq!(repo.artifacts.len(), 1);
    assert_eq!(
        repo.webhooks[0].secret_name.as_deref(),
        Some("jeryu_mirror/webhook/7")
    );
    assert_eq!(
        repo.app_installations[0].token_secret_name.as_deref(),
        Some("jeryu_mirror/app/9/token")
    );
    assert_eq!(
        repo.protected_branches[0].required_status_checks,
        vec!["ci/full"]
    );
}
