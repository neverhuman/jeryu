//! Flight Deck lenses.
//!
//! Invariants: lenses render immutable `*LensInput` projections of
//! [`TuiReadModel`](jeryu_readmodel::TuiReadModel) and never perform backend
//! I/O. Each lens follows the canonical shape: `data` (pure projector) + `view`
//! (pure renderer).
//!
//! This crate ships 9 of the 18 lenses wired end-to-end against the read-model
//! contract (mission/queue/repos + runners/approvals/evidence/agents/release/
//! workflow); [`LensId`] enumerates the full set so the routing surface is
//! stable while the remaining lenses are ported.

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
    pub const IMPLEMENTED: &'static [Self] = &[
        Self::Mission,
        Self::Queue,
        Self::Repos,
        Self::Runners,
        Self::Approvals,
        Self::Evidence,
        Self::Agents,
        Self::Release,
        Self::Workflow,
    ];

    /// Is this lens implemented end-to-end (data + view) in this crate?
    pub fn is_implemented(self) -> bool {
        Self::IMPLEMENTED.contains(&self)
    }

    /// Resolve the lens that owns an [`ActiveTab`] (for the 3 implemented tabs).
    pub fn for_tab(tab: ActiveTab) -> Self {
        match tab {
            ActiveTab::Mission => Self::Mission,
            ActiveTab::Pools | ActiveTab::Jobs => Self::Queue,
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

/// Draw the lens for `id` from the read model into `area`. Implemented lenses
/// project and render; the remaining lenses draw a stable "not yet ported"
/// placeholder so the cockpit never panics on an unported tab.
pub fn draw_lens(f: &mut Frame, id: LensId, model: &TuiReadModel, area: Rect) {
    match id {
        LensId::Mission => {
            mission::view::draw(f, &mission::MissionLensInput::from_read_model(model), area)
        }
        LensId::Queue => queue::view::draw(f, &queue::QueueLensInput::from_read_model(model), area),
        LensId::Repos => repos::view::draw(f, &repos::ReposLensInput::from_read_model(model), area),
        LensId::Runners => {
            runners::view::draw(f, &runners::RunnersLensInput::from_read_model(model), area)
        }
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
        other => draw_placeholder(f, other, area),
    }
}

fn draw_placeholder(f: &mut Frame, id: LensId, area: Rect) {
    use ratatui::widgets::{Block, Borders, Paragraph};
    let text = format!("{} lens — not yet ported in this crate", id.label());
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", id.label())),
        ),
        area,
    );
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
    fn nine_lenses_are_implemented() {
        assert_eq!(LensId::IMPLEMENTED.len(), 9);
        for lens in [
            LensId::Mission,
            LensId::Queue,
            LensId::Repos,
            LensId::Runners,
            LensId::Approvals,
            LensId::Evidence,
            LensId::Agents,
            LensId::Release,
            LensId::Workflow,
        ] {
            assert!(lens.is_implemented(), "{lens:?} should be implemented");
        }
        // Not-yet-ported lenses remain placeholders.
        assert!(!LensId::Bugs.is_implemented());
        assert!(!LensId::Cache.is_implemented());
    }

    #[test]
    fn implemented_tabs_route_to_implemented_lenses() {
        assert_eq!(LensId::for_tab(ActiveTab::Mission), LensId::Mission);
        assert_eq!(LensId::for_tab(ActiveTab::Repos), LensId::Repos);
        assert_eq!(LensId::for_tab(ActiveTab::Pools), LensId::Queue);
    }
}
