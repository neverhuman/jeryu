//! Owner: Interactive TUI subsystem - Flight Deck action orchestration
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: TUI actions route through the shared registry before mutation.

pub use crate::tui::action_registry::{
    ActionEntry, GrantRequirement, RiskTier, SideEffectClass, Surface,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionPhase {
    Preview,
    Confirm,
    Execute,
    Receipt,
}

impl ActionPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Confirm => "confirm",
            Self::Execute => "execute",
            Self::Receipt => "receipt",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_phase_labels_are_stable() {
        assert_eq!(ActionPhase::Preview.label(), "preview");
        assert_eq!(ActionPhase::Receipt.label(), "receipt");
    }
}
