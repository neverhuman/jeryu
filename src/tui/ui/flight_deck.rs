//! Owner: Interactive TUI subsystem — Flight Deck render router
//! Proof: `cargo nextest run -p jeryu --lib tui::ui::flight_deck`
//! Invariants:
//!   - Pure rendering. Maps the active tab to its Flight Deck lens and draws it
//!     from an immutable `TuiReadModel` projection. No backend I/O.
//!   - This is the runtime cutover the reset library was missing: the lenses
//!     under `tui::lenses::*` were built + tested but never rendered by the live
//!     `jeryu tui` loop. `draw_lens` is the single dispatch point that wires a
//!     `LensId` to its `<lens>::draw`, and `tab_lens` maps the existing
//!     `ActiveTab` nav onto the lens set so opening `jeryu tui` shows the new
//!     Flight Deck instead of the legacy panels.
//!   - Tabs without a 1:1 lens (Jobs/Approvals/Git/Secrets/Jankurai) return
//!     `None` and keep their legacy panel until their lens lands — so the cutover
//!     is incremental and never blanks a working screen.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::api::read_model::TuiReadModel;
use crate::tui::app::{ActiveTab, App};
use crate::tui::lenses::{self, LensId};

/// Map the legacy `ActiveTab` nav onto a Flight Deck lens. `None` means "no lens
/// yet — render the legacy panel for this tab". Pools→Runners and Tests→Vti
/// deliberately surface the new-only lenses (which have no legacy tab) through
/// the closest existing tab so they are reachable today.
pub fn tab_lens(tab: ActiveTab) -> Option<LensId> {
    match tab {
        // Route a tab to its lens ONLY when that lens is genuinely built-out —
        // never trade a working legacy panel for a placeholder lens. Today the
        // shipped-quality lenses are Mission (posture cockpit) and Runners (the
        // live multinode pane, surfaced via the Pools tab). Repos is already a
        // lens via its legacy wrapper (which also registers focus). Cache /
        // Evidence / Agents / VTI / Release / Bugs / LLMs lenses are still
        // scaffolds ("lands in U2x") so their richer legacy panels stay until
        // each lens reaches parity.
        ActiveTab::Mission => Some(LensId::Mission),
        ActiveTab::Pools => Some(LensId::Runners),
        ActiveTab::Workflow
        | ActiveTab::Release
        | ActiveTab::Repos
        | ActiveTab::Bugs
        | ActiveTab::LLMs
        | ActiveTab::Tests
        | ActiveTab::Cache
        | ActiveTab::Evidence
        | ActiveTab::Agents
        | ActiveTab::Jobs
        | ActiveTab::Approvals
        | ActiveTab::Git
        | ActiveTab::Secrets
        | ActiveTab::Jankurai => None,
    }
}

/// Render a single lens body into `area`, projecting its input from `model`.
/// `app` is supplied so lenses with a direct app-state adapter (repos) can use
/// the live fleet snapshot rather than the projected read model.
pub fn draw_lens(f: &mut Frame, app: &App, model: &TuiReadModel, lens: LensId, area: Rect) {
    use lenses::*;
    match lens {
        LensId::Mission => {
            mission::draw(f, &mission::MissionLensInput::from_read_model(model), area)
        }
        LensId::Queue => queue::draw(f, &queue::QueueLensInput::from_read_model(model), area),
        LensId::Repos => {
            // Repos has a direct app-state adapter (live fleet snapshot).
            repos::draw(
                f,
                &repos::ReposLensInput::from_fleet_snapshot(&app.state.fleet),
                area,
            )
        }
        LensId::Workflow => workflow::draw(
            f,
            &workflow::WorkflowLensInput::from_read_model(model),
            area,
        ),
        LensId::Evidence => evidence::draw(
            f,
            &evidence::EvidenceLensInput::from_read_model(model),
            area,
        ),
        LensId::SourceDoctor => source_doctor::draw(
            f,
            &source_doctor::SourceDoctorLensInput::from_read_model(model),
            area,
        ),
        LensId::Runners => {
            // Live multinode runner health from the synced node fleet, carrying
            // the pool sync warning forward from the legacy pools tab.
            runners::draw(
                f,
                &runners::RunnersLensInput::from_nodes(&app.state.remote_nodes, model.event_cursor)
                    .with_sync_warning(app.state.pool_sync_error.clone()),
                area,
            )
        }
        LensId::Agents => agents::draw(f, &agents::AgentsLensInput::from_read_model(model), area),
        LensId::Bugs => bugs::draw(f, &bugs::BugsLensInput::from_read_model(model), area),
        LensId::Cache => cache::draw(f, &cache::CacheLensInput::from_read_model(model), area),
        LensId::Vti => vti::draw(f, &vti::VtiLensInput::from_read_model(model), area),
        LensId::Release => {
            release::draw(f, &release::ReleaseLensInput::from_read_model(model), area)
        }
        LensId::Autonomy => autonomy::draw(
            f,
            &autonomy::AutonomyLensInput::from_read_model(model),
            area,
        ),
        LensId::Llms => llms::draw(f, &llms::LlmsLensInput::from_read_model(model), area),
    }
}

/// Best-effort projection of the live `App` (legacy state) into a `TuiReadModel`
/// for the lenses to render. This is the missing app→read-model sync, kept
/// minimal: it fills what the legacy `App` cheaply exposes (runner counts from
/// pools, fleet→repos) and leaves the rest at typed defaults so each lens shows
/// its designed empty/degraded state rather than stale data. The richer live
/// sync (DataClient/inspection plane) is the follow-up; this makes the Flight
/// Deck render today.
pub fn app_to_read_model(app: &App) -> TuiReadModel {
    let mut model = TuiReadModel::default();

    // Runner counts from configured pools (managers desired vs running).
    let total: u32 = app
        .state
        .pools
        .iter()
        .map(|p| p.max_managers.max(0) as u32)
        .sum();
    let active: u32 = app
        .state
        .pools
        .iter()
        .filter(|p| !p.paused)
        .map(|p| p.max_managers.max(0) as u32)
        .sum();
    model.mission.active_runners = active;
    model.mission.total_runners = total;

    // Job posture from tracked pipelines: running = unfinished jobs on running
    // pipelines; failed/queued by pipeline status. Best-effort projection so the
    // Mission and Queue lenses show real local numbers.
    let mut running = 0u32;
    let mut failed = 0u32;
    let mut queued = 0u32;
    for pm in &app.state.pipelines {
        let status = pm.pipeline.status.to_ascii_lowercase();
        match status.as_str() {
            "running" => running += pm.total.saturating_sub(pm.completed) as u32,
            "failed" => failed += 1,
            "pending" | "created" | "waiting_for_resource" | "scheduled" => queued += 1,
            _ => {}
        }
    }
    model.mission.running_jobs = running;
    model.mission.failed_jobs = failed;
    model.mission.queued_jobs = queued;

    // Posture from real signals so the Mission cockpit reflects the live fleet
    // rather than static defaults.
    let unreachable_nodes = app
        .state
        .remote_nodes
        .iter()
        .filter(|n| n.reachable == Some(false))
        .count();
    model.mission.active_agents = app.state.agent_sessions.len() as u32;
    model.mission.safe_to_code = active > 0;
    model.mission.safe_to_merge = failed == 0 && active > 0;
    model.mission.overall = if failed > 0 || unreachable_nodes > 0 {
        crate::api::entity::HealthLevel::Warning
    } else if active == 0 {
        crate::api::entity::HealthLevel::Degraded
    } else {
        crate::api::entity::HealthLevel::Healthy
    };
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pools_tab_surfaces_runners_lens() {
        assert_eq!(tab_lens(ActiveTab::Pools), Some(LensId::Runners));
    }

    #[test]
    fn only_built_out_lenses_are_routed() {
        // Mission cockpit + Runners pane are the shipped-quality lenses.
        assert_eq!(tab_lens(ActiveTab::Mission), Some(LensId::Mission));
        assert_eq!(tab_lens(ActiveTab::Pools), Some(LensId::Runners));
    }

    #[test]
    fn placeholder_and_rich_legacy_tabs_keep_their_panel() {
        // Scaffold lenses (cache/evidence/agents) must NOT replace their richer
        // legacy panels; rich legacy tabs stay until their lens reaches parity.
        for tab in [
            ActiveTab::Cache,
            ActiveTab::Evidence,
            ActiveTab::Agents,
            ActiveTab::Workflow,
            ActiveTab::Repos,
            ActiveTab::Bugs,
            ActiveTab::Jankurai,
        ] {
            assert_eq!(tab_lens(tab), None, "{tab:?} must keep its legacy panel");
        }
    }
}
