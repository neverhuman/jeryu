use super::*;

#[test]
fn repo_summary_uses_repo_entity_kind() {
    let repo = RepoSummary::new("core", "neverhuman/jeryu");
    assert_eq!(repo.entity.kind, EntityKind::Repo);
    assert_eq!(repo.family, "neverhuman");
}

#[test]
fn repos_snapshot_counts_statuses() {
    let mut good = RepoSummary::new("core", "neverhuman/jeryu");
    good.running_count = 1;
    let mut bad = RepoSummary::new("shared", "neverhuman/shared");
    bad.failed_count = 2;
    bad.aged = true;
    let snapshot = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![good, bad]);

    assert_eq!(snapshot.counts(), (1, 2, 1));
    assert_eq!(snapshot.repos_for_family("neverhuman").len(), 2);
    assert_eq!(snapshot.families[0].failed_count, 2);
}

#[test]
fn repos_snapshot_projects_from_fleet_snapshot() {
    let snapshot = crate::repo_fleet::FleetSnapshot {
        generated_at: "2026-05-26T00:00:00Z".into(),
        registry_path: ".jeryu/repos.toml".into(),
        repos: vec![crate::repo_fleet::FleetRepoSnapshot::projection_fixture(
            "core",
            "neverhuman/jeryu",
            true,
        )],
        events: Vec::new(),
    };

    let repos = ReposSnapshot::from_fleet_snapshot(&snapshot);

    assert_eq!(repos.registry_path, ".jeryu/repos.toml");
    assert_eq!(repos.repos[0].entity.kind, EntityKind::Repo);
    assert_eq!(repos.repos[0].family, "neverhuman");
    assert_eq!(repos.repos[0].local_branch.as_deref(), Some("main"));
    assert_eq!(repos.repos[0].score_badge.as_deref(), Some("89"));
    assert!(repos.repos[0].aged);
    assert_eq!(repos.families[0].failed_count, 2);
}

#[test]
fn repo_summary_serializes_aged_freshness() {
    let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
    repo.aged = true;
    let encoded = serde_json::to_value(&repo).expect("serialize repo");

    assert_eq!(encoded["aged"], true);
}

#[test]
fn repos_snapshot_round_trips_json() {
    let snapshot = ReposSnapshot::from_repo_summaries(
        ".jeryu/repos.toml",
        vec![RepoSummary::new("core", "neverhuman/jeryu")],
    );

    let encoded = serde_json::to_string(&snapshot).expect("serialize repos");
    let decoded: ReposSnapshot = serde_json::from_str(&encoded).expect("deserialize repos");
    assert_eq!(decoded, snapshot);
}
