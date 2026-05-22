use super::*;
use crate::bugtracker::AttemptStatus;

fn project(alias: &str) -> BugProjectInput {
    BugProjectInput {
        alias: alias.into(),
        repo_root: format!("/tmp/{alias}"),
        repo_slug: format!("neverhuman/{alias}"),
        provider_kind: "github".into(),
        provider_project_id: None,
        default_branch: "main".into(),
    }
}

fn report() -> CanonicalBugReport {
    CanonicalBugReport {
        target_project: "redlinedb".into(),
        source_project: "veox".into(),
        title: "adapter loses writes".into(),
        component: Some("adapter".into()),
        current_behavior: "writes disappear".into(),
        expected_behavior: "writes persist".into(),
        environment: "local".into(),
        frequency: "always".into(),
        impact: "blocks local agents".into(),
        security_privacy: "none".into(),
        no_secrets_confirmed: true,
        reproduction_steps: vec!["write row".into(), "read row".into()],
        evidence: Vec::new(),
        acceptance_criteria: Vec::new(),
        severity: BugSeverity::S1,
        priority: BugPriority::P1,
        difficulty: 2,
    }
}

#[tokio::test]
async fn submit_list_show_ready_attempts() {
    let repo = BugTrackerRepo::new(fresh_bugtracker_pool().await);
    repo.add_project(&project("veox")).await.unwrap();
    repo.add_project(&project("redlinedb")).await.unwrap();
    repo.link_projects("veox", "redlinedb", "depends_on")
        .await
        .unwrap();
    let bug = repo
        .submit_bug(&report(), Some("idem-1"), "test")
        .await
        .unwrap();
    let same = repo
        .submit_bug(&report(), Some("idem-1"), "test")
        .await
        .unwrap();
    assert_eq!(bug.id, same.id);
    repo.update_bug(
        &bug.id,
        Some(BugStatus::Ready),
        None,
        None,
        None,
        None,
        "triager",
    )
    .await
    .unwrap();
    let ready = repo.ready_bugs(Some("redlinedb")).await.unwrap();
    assert_eq!(ready.len(), 1);
    repo.record_attempt(
        &bug.id,
        &BugAttemptInput {
            agent: Some("codex".into()),
            status: AttemptStatus::Failed,
            sandbox_path: None,
            branch: Some("bug/x".into()),
            base_sha: None,
            head_sha: None,
            pr_url: None,
            ci_evidence: Some("test failed".into()),
            notes: Some("learned thing".into()),
        },
        "codex",
    )
    .await
    .unwrap();
    let detail = repo.show_bug(&bug.id).await.unwrap();
    assert_eq!(detail.events.len(), 3);
    assert_eq!(detail.attempts.len(), 1);
}
