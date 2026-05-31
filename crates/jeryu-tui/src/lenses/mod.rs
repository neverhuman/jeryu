//! Flight Deck lenses.
//!
//! Invariants: lenses render immutable `*LensInput` projections of
//! [`TuiReadModel`](jeryu_readmodel::TuiReadModel) and never perform backend
//! I/O. Each lens follows the canonical shape: `data` (pure projector) + `view`
//! (pure renderer).
//!
//! This crate ships all 18 lenses wired end-to-end against the read-model
//! contract. Dedicated dashboards own the highest-traffic lenses; the remaining
//! operational lenses render live summary panels from the same snapshot.

pub mod agents;
pub mod approvals;
pub mod evidence;
pub mod mission;
pub mod queue;
pub mod release;
pub mod repos;
pub mod runners;
pub mod workflow;

use jeryu_readmodel::TuiReadModel;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::ActiveTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LensId {
    Mission,
    Queue,
    Repos,
    Workflow,
    Evidence,
    SourceDoctor,
    Runners,
    Agents,
    Bugs,
    Cache,
    Vti,
    Release,
    Autonomy,
    Llms,
    Approvals,
    Git,
    Jankurai,
    Secrets,
}

impl LensId {
    pub fn route(self) -> &'static str {
        match self {
            Self::Mission => "mission",
            Self::Queue => "queue",
            Self::Repos => "repos",
            Self::Workflow => "workflow",
            Self::Evidence => "evidence",
            Self::SourceDoctor => "source-doctor",
            Self::Runners => "runners",
            Self::Agents => "agents",
            Self::Bugs => "bugs",
            Self::Cache => "cache",
            Self::Vti => "vti",
            Self::Release => "release",
            Self::Autonomy => "autonomy",
            Self::Llms => "llms",
            Self::Approvals => "approvals",
            Self::Git => "git",
            Self::Jankurai => "jankurai",
            Self::Secrets => "secrets",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mission => "Mission",
            Self::Queue => "Queue",
            Self::Repos => "Repos",
            Self::Workflow => "Workflow",
            Self::Evidence => "Evidence",
            Self::SourceDoctor => "Source Doctor",
            Self::Runners => "Runners",
            Self::Agents => "Agents",
            Self::Bugs => "Bugs",
            Self::Cache => "Cache",
            Self::Vti => "VTI",
            Self::Release => "Release",
            Self::Autonomy => "Autonomy",
            Self::Llms => "LLMs",
            Self::Approvals => "Approvals",
            Self::Git => "Git",
            Self::Jankurai => "Jankurai",
            Self::Secrets => "Secrets",
        }
    }

    pub const CORE: &'static [Self] = &[
        Self::Mission,
        Self::Queue,
        Self::Repos,
        Self::Workflow,
        Self::Evidence,
        Self::SourceDoctor,
        Self::Runners,
        Self::Agents,
        Self::Bugs,
        Self::Cache,
        Self::Vti,
        Self::Release,
        Self::Autonomy,
        Self::Llms,
        Self::Approvals,
        Self::Git,
        Self::Jankurai,
        Self::Secrets,
    ];

    /// The lenses fully wired to the read model in this crate.
    pub const IMPLEMENTED: &'static [Self] = Self::CORE;

    /// Is this lens implemented end-to-end (data + view) in this crate?
    pub fn is_implemented(self) -> bool {
        Self::IMPLEMENTED.contains(&self)
    }

    /// Resolve the lens that owns an [`ActiveTab`] (for the 3 implemented tabs).
    pub fn for_tab(tab: ActiveTab) -> Self {
        match tab {
            ActiveTab::Mission => Self::Mission,
            ActiveTab::Pools | ActiveTab::Jobs => Self::Queue,
            ActiveTab::Runners => Self::Runners,
            ActiveTab::Repos => Self::Repos,
            ActiveTab::Workflow => Self::Workflow,
            ActiveTab::Release => Self::Release,
            ActiveTab::Approvals => Self::Approvals,
            ActiveTab::Agents => Self::Agents,
            ActiveTab::Tests => Self::Vti,
            ActiveTab::Cache => Self::Cache,
            ActiveTab::Evidence => Self::Evidence,
            ActiveTab::Bugs => Self::Bugs,
            ActiveTab::LLMs => Self::Llms,
            ActiveTab::Git => Self::Git,
            ActiveTab::Secrets => Self::Secrets,
            ActiveTab::Jankurai => Self::Jankurai,
        }
    }
}

/// Draw the lens for `id` from the read model into `area`.
pub fn draw_lens(f: &mut Frame, id: LensId, model: &TuiReadModel, area: Rect) {
    match id {
        LensId::Mission => {
            mission::view::draw(f, &mission::MissionLensInput::from_read_model(model), area)
        }
        LensId::Queue => queue::view::draw(f, &queue::QueueLensInput::from_read_model(model), area),
        LensId::Repos => repos::view::draw(f, &repos::ReposLensInput::from_read_model(model), area),
        LensId::Runners => runners::view::draw_from_model(f, model, area),
        LensId::Approvals => approvals::view::draw(
            f,
            &approvals::ApprovalsLensInput::from_read_model(model),
            area,
        ),
        LensId::Evidence => evidence::view::draw(
            f,
            &evidence::EvidenceLensInput::from_read_model(model),
            area,
        ),
        LensId::Agents => {
            agents::view::draw(f, &agents::AgentsLensInput::from_read_model(model), area)
        }
        LensId::Release => {
            release::view::draw(f, &release::ReleaseLensInput::from_read_model(model), area)
        }
        LensId::Workflow => workflow::view::draw(
            f,
            &workflow::WorkflowLensInput::from_read_model(model),
            area,
        ),
        other => draw_summary_lens(f, other, model, area),
    }
}

fn draw_summary_lens(f: &mut Frame, id: LensId, model: &TuiReadModel, area: Rect) {
    let lines = match id {
        LensId::SourceDoctor => {
            let summary = model.source_doctor.summary.clone().unwrap_or_default();
            vec![
                Line::from(format!("Sources total: {}", summary.sources_total)),
                Line::from(format!("Healthy: {}", summary.sources_healthy)),
                Line::from(format!("Degraded: {}", summary.sources_degraded)),
                Line::from(format!("Schema drift: {}", summary.schema_drift_count)),
            ]
        }
        LensId::Bugs => vec![
            Line::from(format!("Attention items: {}", model.attention.len())),
            Line::from(format!("Failed jobs: {}", model.mission.failed_jobs)),
            Line::from(format!("Top blocker: {}", blocker_label(model))),
        ],
        LensId::Cache => vec![
            Line::from(format!(
                "Hit ratio: {:.1}%",
                model.mission.cache_hit_ratio * 100.0
            )),
            Line::from(format!(
                "Selector misses 24h: {}",
                model.mission.selector_misses_24h
            )),
            Line::from(format!("Active taints: {}", model.mission.active_taints)),
        ],
        LensId::Vti => vec![
            Line::from(format!("Running jobs: {}", model.mission.running_jobs)),
            Line::from(format!("Failed jobs: {}", model.mission.failed_jobs)),
            Line::from(format!("Queued jobs: {}", model.mission.queued_jobs)),
        ],
        LensId::Autonomy => vec![
            Line::from(format!("Active agents: {}", model.mission.active_agents)),
            Line::from(format!("Blocked agents: {}", model.mission.blocked_agents)),
            Line::from(format!(
                "Agents can code: {}",
                yes_no(model.mission.agents_can_code)
            )),
        ],
        LensId::Llms => vec![
            Line::from(format!("Active agents: {}", model.mission.active_agents)),
            Line::from(format!("Open capsules: {}", model.mission.open_capsules)),
            Line::from(format!("Active grants: {}", model.mission.active_grants)),
        ],
        LensId::Git => vec![
            Line::from(format!("Repositories: {}", model.repos.repos.len())),
            Line::from(format!("Families: {}", model.repos.families.len())),
            Line::from(format!(
                "Registry: {}",
                empty_dash(&model.repos.registry_path)
            )),
        ],
        LensId::Jankurai => vec![
            Line::from(format!("Evidence count: {}", model.mission.evidence_count)),
            Line::from(format!("Open capsules: {}", model.mission.open_capsules)),
            Line::from(format!(
                "Safe to merge: {}",
                yes_no(model.mission.safe_to_merge)
            )),
        ],
        LensId::Secrets => vec![
            Line::from(format!("Active grants: {}", model.mission.active_grants)),
            Line::from(format!("Active taints: {}", model.mission.active_taints)),
            Line::from(format!(
                "Vault health: {}",
                model.system.vault.status_label()
            )),
        ],
        _ => vec![Line::from("Live summary")],
    };
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", id.label())),
        ),
        area,
    );
}

fn blocker_label(model: &TuiReadModel) -> String {
    model
        .mission
        .top_blocker
        .as_ref()
        .map(|blocker| blocker.summary.clone())
        .unwrap_or_else(|| "none".to_string())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_core_lens_has_a_route_and_label() {
        for lens in LensId::CORE {
            assert!(!lens.route().is_empty());
            assert!(!lens.label().is_empty());
        }
    }

    #[test]
    fn routes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for lens in LensId::CORE {
            assert!(
                seen.insert(lens.route()),
                "duplicate route: {}",
                lens.route()
            );
        }
    }

    #[test]
    fn eighteen_lenses_are_implemented() {
        assert_eq!(LensId::IMPLEMENTED.len(), 18);
        for lens in LensId::CORE {
            assert!(lens.is_implemented(), "{lens:?} should be implemented");
        }
    }

    #[test]
    fn implemented_tabs_route_to_implemented_lenses() {
        assert_eq!(LensId::for_tab(ActiveTab::Mission), LensId::Mission);
        assert_eq!(LensId::for_tab(ActiveTab::Repos), LensId::Repos);
        assert_eq!(LensId::for_tab(ActiveTab::Pools), LensId::Queue);
    }
}
