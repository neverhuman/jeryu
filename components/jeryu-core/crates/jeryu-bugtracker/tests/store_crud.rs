//! CRUD + triage round-trip against the `BugStore` seam (in-memory backend).
//!
//! Mirrors jeryu's `bugtracker_repo_tests.rs` behaviors without a database:
//! submit/list/show/ready/update/record_attempt, idempotency, transition
//! guarding, and the failed-attempt readiness filter.

use jeryu_bugtracker::{
    AttemptStatus, BugAttemptInput, BugPriority, BugProjectInput, BugSeverity, BugSort, BugStatus,
    BugStore, CanonicalBugReport, InMemoryBugStore,
};

fn report(title: &str) -> CanonicalBugReport {
    CanonicalBugReport {
        target_project: "redlinedb".into(),
        source_project: "veox".into(),
        title: title.into(),
        component: Some("adapter".into()),
        current_behavior: "writes disappear".into(),
        expected_behavior: "writes persist".into(),
        environment: "local".into(),
        frequency: "always".into(),
        impact: "blocks local agent state".into(),
        security_privacy: "no security impact".into(),
        no_secrets_confirmed: true,
        reproduction_steps: vec!["submit write".into(), "restart".into()],
        evidence: Vec::new(),
        acceptance_criteria: vec!["write survives restart".into()],
        severity: BugSeverity::S1,
        priority: BugPriority::P1,
        difficulty: 3,
    }
}

#[tokio::test]
async fn project_upsert_is_idempotent_and_lists() {
    let store = InMemoryBugStore::new();
    let input = BugProjectInput {
        alias: "redlinedb".into(),
        repo_root: "/repos/redlinedb".into(),
        repo_slug: "veox/redlinedb".into(),
        provider_kind: "local".into(),
        provider_project_id: None,
        default_branch: "main".into(),
    };
    let p1 = store.add_project(&input).await.unwrap();
    let mut input2 = input.clone();
    input2.default_branch = "trunk".into();
    let p2 = store.add_project(&input2).await.unwrap();
    assert_eq!(p2.default_branch, "trunk");
    assert_eq!(p2.created_at, p1.created_at); // upsert preserves created_at
    assert_eq!(store.list_projects().await.unwrap().len(), 1);
    assert_eq!(
        store.project("redlinedb").await.unwrap().repo_slug,
        "veox/redlinedb"
    );
}

#[tokio::test]
async fn submit_then_show_round_trips() {
    let store = InMemoryBugStore::new();
    let bug = store
        .submit_bug(&report("loses writes"), None, "agent")
        .await
        .unwrap();
    assert!(bug.id.starts_with("bug-"));
    assert_eq!(bug.status, BugStatus::NeedsTriage);
    let detail = store.show_bug(&bug.id).await.unwrap();
    assert_eq!(detail.bug.id, bug.id);
    assert_eq!(detail.events.len(), 1);
    assert_eq!(detail.events[0].event_type, "submitted");
    assert!(detail.attempts.is_empty());
}

#[tokio::test]
async fn submit_honors_idempotency_key() {
    let store = InMemoryBugStore::new();
    let a = store
        .submit_bug(&report("dup"), Some("key-1"), "agent")
        .await
        .unwrap();
    let b = store
        .submit_bug(&report("dup"), Some("key-1"), "agent")
        .await
        .unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(
        store
            .list_bugs(None, None, BugSort::Rank)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn list_filters_by_project_and_status() {
    let store = InMemoryBugStore::new();
    store
        .submit_bug(&report("one"), None, "agent")
        .await
        .unwrap();
    let mut other = report("two");
    other.target_project = "other".into();
    store.submit_bug(&other, None, "agent").await.unwrap();

    let all = store.list_bugs(None, None, BugSort::Rank).await.unwrap();
    assert_eq!(all.len(), 2);
    let only = store
        .list_bugs(Some("redlinedb"), None, BugSort::Rank)
        .await
        .unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].target_project, "redlinedb");
    let none = store
        .list_bugs(None, Some(BugStatus::Done), BugSort::Rank)
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn update_advances_status_and_blocks_terminal_reopen() {
    let store = InMemoryBugStore::new();
    let bug = store
        .submit_bug(&report("triage me"), None, "agent")
        .await
        .unwrap();
    let accepted = store
        .update_bug(
            &bug.id,
            Some(BugStatus::Accepted),
            None,
            None,
            None,
            Some("alice"),
            "lead",
        )
        .await
        .unwrap();
    assert_eq!(accepted.status, BugStatus::Accepted);
    assert_eq!(accepted.owner.as_deref(), Some("alice"));

    let done = store
        .update_bug(
            &bug.id,
            Some(BugStatus::Done),
            None,
            None,
            None,
            None,
            "lead",
        )
        .await
        .unwrap();
    assert_eq!(done.status, BugStatus::Done);

    // Terminal -> reopen must fail.
    let reopen = store
        .update_bug(
            &bug.id,
            Some(BugStatus::Ready),
            None,
            None,
            None,
            None,
            "lead",
        )
        .await;
    assert!(reopen.is_err());
}

#[tokio::test]
async fn ready_filters_out_bugs_with_three_failed_attempts() {
    let store = InMemoryBugStore::new();
    let bug = store
        .submit_bug(&report("ready path"), None, "agent")
        .await
        .unwrap();
    store
        .update_bug(
            &bug.id,
            Some(BugStatus::Ready),
            None,
            None,
            None,
            None,
            "lead",
        )
        .await
        .unwrap();
    assert_eq!(store.ready_bugs(None).await.unwrap().len(), 1);

    for _ in 0..3 {
        store
            .record_attempt(
                &bug.id,
                &BugAttemptInput {
                    agent: Some("fixer".into()),
                    status: AttemptStatus::Failed,
                    sandbox_path: None,
                    branch: Some("bug/fix".into()),
                    base_sha: None,
                    head_sha: None,
                    pr_url: Some("https://jeryu.example/pr/7".into()),
                    ci_evidence: None,
                    notes: None,
                },
                "fixer",
            )
            .await
            .unwrap();
    }

    let detail = store.show_bug(&bug.id).await.unwrap();
    assert_eq!(detail.bug.attempt_count, 3);
    assert_eq!(detail.bug.failed_attempt_count, 3);
    assert_eq!(detail.attempts.len(), 3);
    // 3 failed attempts -> filtered out of the ready queue.
    assert!(store.ready_bugs(None).await.unwrap().is_empty());
}

#[tokio::test]
async fn record_attempt_on_unknown_bug_errors() {
    let store = InMemoryBugStore::new();
    let res = store
        .record_attempt(
            "bug-deadbeef00",
            &BugAttemptInput {
                agent: None,
                status: AttemptStatus::Started,
                sandbox_path: None,
                branch: None,
                base_sha: None,
                head_sha: None,
                pr_url: None,
                ci_evidence: None,
                notes: None,
            },
            "agent",
        )
        .await;
    assert!(res.is_err());
}
