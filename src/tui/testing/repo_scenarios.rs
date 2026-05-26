use crate::{
    api::read_model::{RepoSummary, ReposSnapshot, TuiReadModel},
    tui::testing::FixtureScenario,
};

pub(super) fn apply_repo_fixture(scenario: FixtureScenario, model: &mut TuiReadModel) {
    model.repos = match scenario {
        FixtureScenario::Empty => ReposSnapshot::from_repo_summaries("fixture:empty", Vec::new()),
        FixtureScenario::Stale | FixtureScenario::Aged => {
            let mut repo = repo("core", "neverhuman/jeryu", "aged");
            repo.aged = true;
            let mut snapshot = ReposSnapshot::from_repo_summaries("fixture:aged", vec![repo]);
            if let Some(family) = snapshot.families.first_mut() {
                family.status = "aged".into();
            }
            snapshot
        }
        FixtureScenario::Degraded => {
            let mut core = repo("core", "neverhuman/jeryu", "failed");
            core.failed_count = 2;
            let mut shared = repo("shared", "neverhuman/shared", "green");
            shared.running_count = 1;
            ReposSnapshot::from_repo_summaries("fixture:degraded", vec![core, shared])
        }
        FixtureScenario::SourceDown => {
            let mut repo = repo("core", "neverhuman/jeryu", "source_down");
            repo.next_command = "retry source connection".into();
            let mut snapshot =
                ReposSnapshot::from_repo_summaries("fixture:source_down", vec![repo]);
            if let Some(family) = snapshot.families.first_mut() {
                family.status = "source_down".into();
            }
            snapshot
        }
        _ => {
            let mut core = repo("core", "neverhuman/jeryu", "running");
            core.running_count = 1;
            let shared = repo("shared", "neverhuman/shared", "green");
            ReposSnapshot::from_repo_summaries("fixture:healthy", vec![core, shared])
        }
    };
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
