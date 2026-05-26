use chrono::Utc;
use ratatui::{buffer::Buffer, widgets::Widget};

use super::*;
use crate::{
    api::{
        entity::{ActionRef, EntityKind, EntityRef, Severity},
        read_model::{AttentionItem, RepoFamilySummary, RepoSummary, TuiReadModel},
    },
    tui::{
        app::{
            reducer::AppIntent,
            state::{AppRoute, FlightDeckState},
        },
        theme::{TerminalCaps, Theme},
        widgets::shared::CanonicalSize,
    },
};

fn render_text(size: CanonicalSize) -> String {
    let fixture = ReposFixture::default();
    render_model_text(size, &fixture.model)
}

fn render_model_text(size: CanonicalSize, model: &TuiReadModel) -> String {
    let selection = ReposSelection::default();
    let theme = Theme::dark();
    let area = size.area();
    let mut buffer = Buffer::empty(area);
    let input = select_repos_lens_input(model, &selection);
    ReposLens::new(input, &theme, TerminalCaps::ascii()).render(area, &mut buffer);
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn family_summaries_can_be_derived_from_repos() {
    let mut fixture = ReposFixture::default();
    fixture.model.repos.families.clear();
    let input = fixture.input();
    let families = input.families();

    assert_eq!(families.len(), 2);
    assert!(families.iter().any(|family| family.name == "neverhuman"));
    assert!(families.iter().any(|family| family.failed_count == 1));
}

#[test]
fn family_selection_filters_repositories() {
    let mut fixture = ReposFixture::default();
    fixture.selection.family = Some("neverhuman".into());
    let repos = fixture.input().repos();

    assert_eq!(repos.len(), 2);
    assert!(repos.iter().all(|repo| repo.family == "neverhuman"));
}

#[test]
fn repo_selection_scopes_attention() {
    let mut fixture = ReposFixture::default();
    fixture.model.attention.push(attention(
        "a3",
        EntityRef::new(EntityKind::Job, "job/shared-name-only"),
        "shared text but different entity",
        Utc::now(),
    ));
    fixture.selection.repo = Some("shared".into());
    let attention = fixture.input().scoped_attention();

    assert_eq!(attention.len(), 1);
    assert_eq!(attention[0].entity.id, "neverhuman/shared");
}

#[test]
fn nav_activation_returns_route_intents() {
    let mut fixture = ReposFixture::default();
    fixture.selection.family = Some("neverhuman".into());
    fixture.selection.repo = Some("shared".into());
    let state = FlightDeckState::default();

    assert_eq!(
        activate_pane(ReposPane::Families, fixture.input(), &state),
        ReposNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(EntityRef::new(
            EntityKind::RepoFamily,
            "family/neverhuman"
        ))))
    );
    assert_eq!(
        activate_pane(ReposPane::Repos, fixture.input(), &state),
        ReposNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(EntityRef::new(
            EntityKind::Repo,
            "neverhuman/shared"
        ))))
    );
}

#[test]
fn repos_lens_renders_at_canonical_sizes() {
    for size in [
        CanonicalSize::Compact,
        CanonicalSize::Standard,
        CanonicalSize::Wide,
    ] {
        let text = render_text(size);
        assert!(text.contains("Repo"));
    }

    let wide = render_text(CanonicalSize::Wide);
    assert!(wide.contains("Repository Fleet"));
    assert!(wide.contains("Families"));
    assert!(wide.contains("Repositories"));
    assert!(wide.contains("Scoped Attention"));
}

#[test]
fn repos_lens_renders_degradation_fixture_states() {
    for (scenario, expected) in [
        (
            crate::tui::testing::FixtureScenario::Empty,
            "No repositories",
        ),
        (crate::tui::testing::FixtureScenario::Aged, "aged"),
        (crate::tui::testing::FixtureScenario::Degraded, "failed"),
        (
            crate::tui::testing::FixtureScenario::SourceDown,
            "source_down",
        ),
    ] {
        let fixture = crate::tui::testing::ScenarioFixture::build(scenario);
        let text = render_model_text(CanonicalSize::Wide, &fixture.read_model);
        assert!(
            text.contains(expected),
            "{scenario:?} fixture should render {expected:?}; text={text}"
        );
    }
}

#[test]
fn nav_focus_moves_between_repos_panes() {
    assert_eq!(
        move_focus(ReposPane::Fleet, crate::tui::nav::NavDirection::Right),
        ReposNavOutcome::Focus(ReposPane::Families)
    );
    assert_eq!(
        move_focus(ReposPane::Attention, crate::tui::nav::NavDirection::Down),
        ReposNavOutcome::Focus(ReposPane::Attention)
    );
}

struct ReposFixture {
    model: TuiReadModel,
    selection: ReposSelection,
}

impl Default for ReposFixture {
    fn default() -> Self {
        let now = Utc::now();
        let core = repo("core", "neverhuman/jeryu", "green", 0, 0);
        let shared = repo("shared", "neverhuman/shared", "failed", 0, 1);
        let tools = repo("tools", "ops/tools", "running", 2, 0);
        let mut model = TuiReadModel::default();
        model.repos.registry_path = ".jeryu/repos.toml".into();
        model.repos.families = vec![
            RepoFamilySummary {
                repo_count: 2,
                failed_count: 1,
                status: "failed".into(),
                ..RepoFamilySummary::new("neverhuman")
            },
            RepoFamilySummary {
                repo_count: 1,
                running_count: 2,
                status: "running".into(),
                ..RepoFamilySummary::new("ops")
            },
        ];
        model.repos.repos = vec![core, shared, tools];
        model.attention = vec![
            attention(
                "a1",
                EntityRef::new(EntityKind::Repo, "neverhuman/shared"),
                "shared failed",
                now,
            ),
            attention(
                "a2",
                EntityRef::new(EntityKind::Repo, "ops/tools"),
                "tools running",
                now,
            ),
        ];
        Self {
            model,
            selection: ReposSelection::default(),
        }
    }
}

impl ReposFixture {
    fn input(&self) -> ReposLensInput<'_> {
        select_repos_lens_input(&self.model, &self.selection)
    }
}

fn repo(alias: &str, slug: &str, status: &str, running: u32, failed: u32) -> RepoSummary {
    let mut repo = RepoSummary::new(alias, slug);
    repo.provider = "github".into();
    repo.status = status.into();
    repo.running_count = running;
    repo.failed_count = failed;
    repo.local_branch = Some("main".into());
    repo.local_sha = Some("abc1234".into());
    repo.next_command = "just fast".into();
    repo
}

fn attention(
    id: &str,
    entity: EntityRef,
    title: &str,
    now: chrono::DateTime<Utc>,
) -> AttentionItem {
    AttentionItem {
        id: id.into(),
        severity: Severity::Error,
        title: title.into(),
        why_it_matters: title.into(),
        entity,
        evidence: vec![format!("proof/{id}")],
        recommended_actions: vec![ActionRef {
            action_id: "open_logs".into(),
            label: "Open logs".into(),
            risk: None,
        }],
        created_at: now,
        last_seen_at: now,
    }
}
