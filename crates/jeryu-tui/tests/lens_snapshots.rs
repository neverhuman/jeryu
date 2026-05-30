//! tuiwright-style snapshot tests.
//!
//! Each test renders one of the 3 implemented lenses from a fixture
//! [`TuiReadModel`] through the full chrome (`render_once`) and asserts key
//! content of the flattened cell text. This proves the read-model → TUI rewire:
//! the lenses are pure projections of `jeryu-readmodel`, and the runtime
//! composes them with the header/status-strip chrome without any backend I/O.

use jeryu_readmodel::{
    EntityKind, EntityRef, RepoSummary, ReposSnapshot, TuiReadModel, TuiReadModelBuilder,
    sample_read_model,
};
use jeryu_tui::{ActiveTab, App, StreamMode, render_once};

/// Render a single Flight Deck frame for `tab` from `model`.
fn snapshot(model: TuiReadModel, tab: ActiveTab) -> String {
    let mut app = App::new_render_only(model);
    app.set_tab(tab);
    render_once(&app, 120, 40, StreamMode::Fixture)
}

// ── Mission lens ─────────────────────────────────────────────────────────

#[test]
fn mission_lens_renders_posture_attention_and_next_action() {
    let ink = snapshot(sample_read_model(), ActiveTab::Mission);

    // Chrome: brand, active tab, fixture stream badge, status hints.
    assert!(ink.contains("jeryu"), "header brand missing");
    assert!(ink.contains("Mission"), "tab label missing");
    assert!(ink.contains("FIXTURE"), "stream badge missing");

    // Mission lens content projected from the read model.
    assert!(ink.contains("Mission Control"), "posture panel missing");
    assert!(ink.contains("Posture"), "posture panel title missing");
    assert!(ink.contains("Attention"), "attention panel missing");
    assert!(ink.contains("Next Action"), "next-action panel missing");

    // Fixture data surfaced: the sample model has a build-web failure + retry.
    assert!(
        ink.contains("build-web failing"),
        "attention item not projected"
    );
    assert!(ink.contains("Retry"), "next action label not projected");
    // Sample posture is Warning and not safe to merge.
    assert!(ink.contains("Warning"), "overall posture not projected");
}

#[test]
fn mission_lens_default_model_shows_empty_states() {
    let ink = snapshot(TuiReadModel::default(), ActiveTab::Mission);
    assert!(ink.contains("(no attention items)"));
    assert!(ink.contains("(no recommendation)"));
    // Default posture is safe to code.
    assert!(ink.contains("Safe to code: yes"));
}

// ── Queue lens ──────────────────────────────────────────────────────────

#[test]
fn queue_lens_renders_capacity_and_headroom_from_read_model() {
    let mut model = TuiReadModel {
        event_cursor: 77,
        ..Default::default()
    };
    model.mission.queued_jobs = 12;
    model.mission.running_jobs = 4;
    model.mission.failed_jobs = 2;
    model.mission.active_runners = 6;
    model.mission.total_runners = 8;

    // Queue maps to the Pools tab in this crate's tab routing.
    let ink = snapshot(model, ActiveTab::Pools);

    assert!(ink.contains("Queue"), "queue header missing");
    assert!(ink.contains("Capacity"), "capacity table missing");
    assert!(ink.contains("Headroom"), "headroom row missing");
    assert!(ink.contains("12 queued"), "queued count not projected");
    assert!(ink.contains("utilization"), "utilization not derived");
    // queue (12) exceeds headroom (8-6=2) → warning copy.
    assert!(
        ink.contains("exceeds free capacity"),
        "headroom-overflow warning not rendered"
    );
    assert!(ink.contains("cursor=77"), "event cursor not projected");
}

#[test]
fn queue_lens_default_model_is_idle() {
    let ink = snapshot(TuiReadModel::default(), ActiveTab::Pools);
    assert!(ink.contains("Queue"));
    assert!(ink.contains("0 queued"));
    assert!(ink.contains("Headroom"));
}

// ── Repos lens ──────────────────────────────────────────────────────────

#[test]
fn repos_lens_renders_fleet_families_and_detail_from_read_model() {
    let mut model = TuiReadModel {
        event_cursor: 9012,
        ..Default::default()
    };
    let mut repo = RepoSummary::new("core", "neverhuman/jeryu");
    repo.status = "running".into();
    repo.running_count = 1;
    repo.entity = EntityRef::new(EntityKind::Repo, "neverhuman/jeryu");
    model.repos = ReposSnapshot::from_repo_summaries(".jeryu/repos.toml", vec![repo]);

    let ink = snapshot(model, ActiveTab::Repos);

    assert!(ink.contains("Repository Fleet"), "fleet panel missing");
    assert!(ink.contains("Families"), "families panel missing");
    assert!(ink.contains("Repositories"), "repos panel missing");
    assert!(ink.contains("Repo Detail"), "detail panel missing");
    assert!(ink.contains("Scoped Attention"), "attention panel missing");
    // Projected family + repo content.
    assert!(ink.contains("neverhuman"), "family name not projected");
    assert!(ink.contains("core"), "repo alias not projected");
    assert!(
        ink.contains(".jeryu/repos.toml"),
        "registry path not projected"
    );
}

#[test]
fn repos_lens_default_model_shows_empty_fleet() {
    let ink = snapshot(TuiReadModel::default(), ActiveTab::Repos);
    assert!(ink.contains("Repository Fleet"));
    assert!(ink.contains("No repositories"));
    assert!(ink.contains("No repo families"));
}

// ── Chrome / routing invariants ───────────────────────────────────────────

#[test]
fn builder_fixture_round_trips_into_a_renderable_frame() {
    let model = TuiReadModelBuilder::new().event_cursor(5).build();
    let ink = snapshot(model, ActiveTab::Mission);
    assert!(ink.contains("Cursor: 5"));
}

#[test]
fn unported_tab_renders_placeholder_not_a_panic() {
    let ink = snapshot(sample_read_model(), ActiveTab::Workflow);
    assert!(ink.contains("not yet ported"));
    // Chrome still present.
    assert!(ink.contains("jeryu"));
    assert!(ink.contains("Workflow"));
}

#[test]
fn stream_mode_badge_reflects_transport() {
    let mut app = App::new_render_only(TuiReadModel::default());
    app.set_tab(ActiveTab::Mission);
    assert!(render_once(&app, 120, 40, StreamMode::Live).contains("LIVE"));
    assert!(render_once(&app, 120, 40, StreamMode::Poll).contains("[poll]"));
    assert!(render_once(&app, 120, 40, StreamMode::Degraded).contains("DEGRADED"));
}
