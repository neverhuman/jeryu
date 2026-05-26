use ratatui::{buffer::Buffer, widgets::Widget};

use super::*;
use crate::{
    api::read_model::{
        MissionSnapshot, QueueJobSummary, QueuePoolSnapshot, QueueSnapshot, RunnerHealth,
        SystemHealth, TuiReadModel,
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
    let fixture = QueueFixture::saturated();
    let theme = Theme::dark();
    let area = size.area();
    let mut buffer = Buffer::empty(area);
    QueueLens::new(fixture.input(), &theme, TerminalCaps::ascii()).render(area, &mut buffer);
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn saturated_queue_says_adding_runners_helps() {
    let fixture = QueueFixture::saturated();
    let capacity = analyze_capacity(fixture.input());
    let delta = runner_delta(fixture.input(), 1);

    assert_eq!(capacity.queued_jobs, 2);
    assert_eq!(capacity.active_parallel_limit, 2);
    assert_eq!(capacity.configured_max_slots, 3);
    assert_eq!(capacity.add_runner_effect, AddRunnerEffect::Helpful);
    assert_eq!(delta.jobs_unblocked, 1);
}

#[test]
fn idle_capacity_says_scheduler_is_the_limit() {
    let fixture = QueueFixture::new(QueueSnapshot {
        total_waiting_jobs: 1,
        total_running_jobs: 1,
        pools: vec![pool("trusted", 4, 4, 1, 1, &["linux"], false)],
        waiting_jobs: vec![job("build-web", "pending", 12, &["linux"], "build")],
    });

    let capacity = analyze_capacity(fixture.input());
    assert_eq!(capacity.add_runner_effect, AddRunnerEffect::IdleCapacity);
    assert_eq!(runner_delta(fixture.input(), 2).jobs_unblocked, 0);
}

#[test]
fn tag_bound_queue_blocks_green_capacity_claims() {
    let fixture = QueueFixture::new(QueueSnapshot {
        total_waiting_jobs: 1,
        total_running_jobs: 2,
        pools: vec![pool("trusted", 2, 3, 1, 2, &["linux"], false)],
        waiting_jobs: vec![job("secret-scan", "pending", 44, &["security"], "security")],
    });

    let capacity = analyze_capacity(fixture.input());
    assert_eq!(capacity.add_runner_effect, AddRunnerEffect::TagBound);
    assert_eq!(capacity.status_key(), "blocked");
}

#[test]
fn paused_capacity_is_called_out_before_buying_runners() {
    let fixture = QueueFixture::new(QueueSnapshot {
        total_waiting_jobs: 1,
        total_running_jobs: 2,
        pools: vec![
            pool("trusted", 2, 2, 1, 2, &["linux"], false),
            pool("warm", 0, 2, 1, 0, &["linux"], true),
        ],
        waiting_jobs: vec![job("build-web", "pending", 30, &["linux"], "build")],
    });

    assert_eq!(
        analyze_capacity(fixture.input()).add_runner_effect,
        AddRunnerEffect::PausedCapacity
    );
}

#[test]
fn configured_limit_is_not_reported_as_helpful() {
    let fixture = QueueFixture::new(QueueSnapshot {
        total_waiting_jobs: 3,
        total_running_jobs: 2,
        pools: vec![pool("trusted", 2, 2, 1, 2, &["linux"], false)],
        waiting_jobs: vec![
            job("build-web", "pending", 30, &["linux"], "build"),
            job("build-api", "pending", 20, &["linux"], "build"),
            job("build-cli", "pending", 10, &["linux"], "build"),
        ],
    });

    let capacity = analyze_capacity(fixture.input());
    assert_eq!(
        capacity.add_runner_effect,
        AddRunnerEffect::AtConfiguredLimit
    );
    assert_eq!(runner_delta(fixture.input(), 1).jobs_unblocked, 0);
}

#[test]
fn source_limited_queue_never_renders_green() {
    let fixture = QueueFixture::new(QueueSnapshot {
        total_waiting_jobs: 1,
        total_running_jobs: 0,
        pools: Vec::new(),
        waiting_jobs: vec![job("build-web", "pending", 30, &[], "build")],
    });

    assert_eq!(
        analyze_capacity(fixture.input()).add_runner_effect,
        AddRunnerEffect::SourceLimited
    );
}

#[test]
fn stage_summaries_average_queue_age() {
    let fixture = QueueFixture::saturated();
    let summaries = fixture.input().stage_summaries();
    let build = summaries
        .iter()
        .find(|summary| summary.stage == "build")
        .expect("build stage");

    assert_eq!(build.queued, 2);
    assert_eq!(build.avg_queue_secs, 45);
}

#[test]
fn nav_activation_returns_intents_without_mutating_state() {
    let fixture = QueueFixture::saturated();
    let state = FlightDeckState::default();

    assert_eq!(
        activate_pane(QueuePane::Jobs, fixture.input(), &state),
        QueueNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(
            fixture.model.queue.waiting_jobs[0].entity.clone()
        )))
    );
    assert!(matches!(
        activate_pane(QueuePane::Pools, fixture.input(), &state),
        QueueNavOutcome::Intent(AppIntent::SelectEntity(Some(_)))
    ));
}

#[test]
fn queue_lens_renders_at_canonical_sizes() {
    for size in [
        CanonicalSize::Compact,
        CanonicalSize::Standard,
        CanonicalSize::Wide,
    ] {
        let text = render_text(size);
        assert!(text.contains("Queue"));
    }

    let wide = render_text(CanonicalSize::Wide);
    assert!(wide.contains("ADDING RUNNERS HELPS"));
    assert!(wide.contains("Waiting Jobs"));
    assert!(wide.contains("Pool Fit"));
}

#[test]
fn nav_focus_moves_between_queue_panes() {
    assert_eq!(
        move_focus(QueuePane::Capacity, crate::tui::nav::NavDirection::Right),
        QueueNavOutcome::Focus(QueuePane::Lab)
    );
    assert_eq!(
        move_focus(QueuePane::Pools, crate::tui::nav::NavDirection::Down),
        QueueNavOutcome::Focus(QueuePane::Pools)
    );
}

struct QueueFixture {
    model: TuiReadModel,
}

impl QueueFixture {
    fn saturated() -> Self {
        Self::new(QueueSnapshot {
            total_waiting_jobs: 2,
            total_running_jobs: 2,
            pools: vec![pool("trusted", 2, 3, 1, 2, &["linux"], false)],
            waiting_jobs: vec![
                job("build-web", "pending", 60, &["linux"], "build"),
                job("build-api", "waiting_for_resource", 30, &["linux"], "build"),
            ],
        })
    }

    fn new(queue: QueueSnapshot) -> Self {
        let total_slots: u32 = queue
            .pools
            .iter()
            .map(QueuePoolSnapshot::active_slots)
            .sum();
        let busy: u32 = queue.pools.iter().map(|pool| pool.running_jobs).sum();
        let idle = total_slots.saturating_sub(busy);
        let mut model = TuiReadModel::default();
        model.mission = MissionSnapshot {
            queued_jobs: queue.total_waiting_jobs,
            running_jobs: queue.total_running_jobs,
            active_runners: busy,
            total_runners: total_slots,
            ..MissionSnapshot::default()
        };
        model.system = SystemHealth {
            runners: RunnerHealth {
                online: total_slots,
                busy,
                idle,
                degraded: 0,
            },
            ..SystemHealth::default()
        };
        model.queue = queue;
        Self { model }
    }

    fn input(&self) -> QueueLensInput<'_> {
        select_queue_lens_input(&self.model)
    }
}

fn pool(
    name: &str,
    active_managers: u32,
    max_managers: u32,
    slots_per_manager: u32,
    running_jobs: u32,
    tags: &[&str],
    paused: bool,
) -> QueuePoolSnapshot {
    let mut pool = QueuePoolSnapshot::new(name);
    pool.active_managers = active_managers;
    pool.max_managers = max_managers;
    pool.slots_per_manager = slots_per_manager;
    pool.running_jobs = running_jobs;
    pool.tags = tags.iter().map(|tag| (*tag).to_string()).collect();
    pool.paused = paused;
    pool
}

fn job(label: &str, status: &str, queued_secs: u64, tags: &[&str], stage: &str) -> QueueJobSummary {
    let mut job = QueueJobSummary::new(label, queued_secs * 1_000);
    job.status = status.into();
    job.stage = stage.into();
    job.required_tags = tags.iter().map(|tag| (*tag).to_string()).collect();
    job
}
