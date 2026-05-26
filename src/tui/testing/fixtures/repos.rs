//! Owner: Interactive TUI subsystem — repos scenario fixtures (U18)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::repos`
//! Invariants: Deterministic; pure data; no I/O.

use crate::api::entity::{HealthLevel, Severity};
use crate::api::read_model::{
    AttentionItem, MissionSnapshot, RepoSummary, ReposSnapshot, TuiReadModel,
};

use super::{fresh, healthy_system, ts};

pub fn healthy() -> TuiReadModel {
    let mut m = base("fixture:repos:healthy");
    let mut core = repo("core", "neverhuman/jeryu", "running");
    core.running_count = 1;
    let shared = repo("shared", "neverhuman/shared", "green");
    m.repos = ReposSnapshot::from_repo_summaries("fixture:repos:healthy", vec![core, shared]);
    m
}

pub fn empty() -> TuiReadModel {
    let mut m = base("fixture:repos:empty");
    m.repos = ReposSnapshot::from_repo_summaries("fixture:repos:empty", Vec::new());
    m
}

pub fn aged() -> TuiReadModel {
    let mut m = base("fixture:repos:aged");
    let mut core = repo("core", "neverhuman/jeryu", "aged");
    core.aged = true;
    m.repos = ReposSnapshot::from_repo_summaries("fixture:repos:aged", vec![core.clone()]);
    add_repo_attention(&mut m, Severity::Warning, "Repo data aged", &core.slug);
    m
}

pub fn degraded() -> TuiReadModel {
    let mut m = base("fixture:repos:degraded");
    m.mission.overall = HealthLevel::Degraded;
    let mut core = repo("core", "neverhuman/jeryu", "failed");
    core.failed_count = 2;
    let shared = repo("shared", "neverhuman/shared", "green");
    m.repos =
        ReposSnapshot::from_repo_summaries("fixture:repos:degraded", vec![core.clone(), shared]);
    add_repo_attention(&mut m, Severity::Error, "Repo jobs failed", &core.slug);
    m
}

pub fn source_down() -> TuiReadModel {
    let mut m = base("fixture:repos:source_down");
    m.mission.overall = HealthLevel::Critical;
    m.freshness.overall_stale = true;
    let mut core = repo("core", "neverhuman/jeryu", "source_down");
    core.next_command = "retry source connection".into();
    m.repos = ReposSnapshot::from_repo_summaries("fixture:repos:source_down", vec![core.clone()]);
    add_repo_attention(&mut m, Severity::Critical, "Repo source down", &core.slug);
    m
}

fn base(path: &str) -> TuiReadModel {
    let mut m = TuiReadModel::default();
    m.generated_at = ts(13, 20, 0);
    m.event_cursor = 520;
    m.freshness = fresh(1_000, 800, 1_200, 900, 1_500, false);
    m.mission = MissionSnapshot {
        overall: HealthLevel::Healthy,
        safe_to_code: true,
        safe_to_merge: true,
        active_agents: 2,
        active_runners: 12,
        total_runners: 12,
        evidence_count: 10,
        ..MissionSnapshot::default()
    };
    m.system = healthy_system(2, 10);
    m.repos.registry_path = path.into();
    m
}

fn repo(alias: &str, slug: &str, status: &str) -> RepoSummary {
    let mut repo = RepoSummary::new(alias, slug);
    repo.provider = "github".into();
    repo.default_branch = "main".into();
    repo.visibility = "private".into();
    repo.health_profile = "rust-workspace".into();
    repo.status = status.into();
    repo.local_branch = Some("main".into());
    repo.local_sha = Some("abc1234".into());
    repo.next_command = "just fast".into();
    repo
}

fn add_repo_attention(m: &mut TuiReadModel, severity: Severity, title: &str, slug: &str) {
    m.attention.push(AttentionItem {
        id: format!("att-repos-{}", title.replace(' ', "-").to_ascii_lowercase()),
        severity,
        title: title.into(),
        why_it_matters: "Repo state changes lens confidence.".into(),
        entity: crate::api::entity::EntityRef::new(crate::api::entity::EntityKind::Repo, slug),
        evidence: vec!["proof/repos".into()],
        recommended_actions: Vec::new(),
        created_at: ts(13, 19, 0),
        last_seen_at: ts(13, 20, 0),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dets(m: &TuiReadModel) -> String {
        serde_json::to_string(m).unwrap()
    }

    #[test]
    fn repo_fixtures_are_deterministic() {
        assert_eq!(dets(&healthy()), dets(&healthy()));
        assert_eq!(dets(&empty()), dets(&empty()));
        assert_eq!(dets(&aged()), dets(&aged()));
        assert_eq!(dets(&degraded()), dets(&degraded()));
        assert_eq!(dets(&source_down()), dets(&source_down()));
    }

    #[test]
    fn repo_fixtures_cover_degradation_states() {
        assert!(empty().repos.repos.is_empty());
        assert!(aged().repos.repos.iter().any(|repo| repo.aged));
        assert!(
            degraded()
                .repos
                .repos
                .iter()
                .any(|repo| repo.failed_count > 0)
        );
        assert!(
            source_down()
                .repos
                .repos
                .iter()
                .any(|repo| repo.status == "source_down")
        );
    }
}
