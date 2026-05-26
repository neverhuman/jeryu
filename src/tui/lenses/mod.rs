//! Owner: Interactive TUI subsystem - Flight Deck lenses
//! Proof: `cargo nextest run -p jeryu --lib tui::lenses::`
//! Invariants: Lenses render immutable LensInput projections and never perform
//!             backend I/O. Each lens follows the canonical 5-file shape:
//!             mod.rs / view.rs / data.rs / nav.rs / tests.rs.

pub mod agents;
pub mod bugs;
pub mod evidence;
pub mod mission;
pub mod queue;
pub mod repos;
pub mod runners;
pub mod source_doctor;
pub mod workflow;

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
    ];
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
            assert!(seen.insert(lens.route()), "duplicate route: {}", lens.route());
        }
    }
}
