//! Owner: Interactive TUI subsystem - Flight Deck lenses
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: Lenses render immutable inputs and never perform backend I/O.

pub mod evidence;
pub mod mission;
pub mod queue;
pub mod repos;
pub mod workflow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LensId {
    Mission,
    Queue,
    Repos,
    Workflow,
    Evidence,
    SourceDoctor,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_lenses_have_routes() {
        for lens in [
            LensId::Mission,
            LensId::Queue,
            LensId::Repos,
            LensId::Workflow,
            LensId::Evidence,
            LensId::SourceDoctor,
        ] {
            assert!(!lens.route().is_empty());
        }
    }
}
