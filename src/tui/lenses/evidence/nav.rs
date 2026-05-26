use crate::tui::{
    app::{
        reducer::AppIntent,
        state::{AppRoute, FlightDeckState},
    },
    lenses::evidence::{EvidenceLensInput, build_entity_proof_graph},
    nav::NavDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidencePane {
    Search,
    Timeline,
    Graph,
    Receipts,
    Bundle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceNavOutcome {
    Focus(EvidencePane),
    Intent(AppIntent),
    None,
}

pub fn move_focus(current: EvidencePane, direction: NavDirection) -> EvidenceNavOutcome {
    let order = [
        EvidencePane::Search,
        EvidencePane::Timeline,
        EvidencePane::Graph,
        EvidencePane::Receipts,
        EvidencePane::Bundle,
    ];
    let current_index = order
        .iter()
        .position(|pane| *pane == current)
        .unwrap_or_default();
    let next_index = match direction {
        NavDirection::Up | NavDirection::Left => current_index.saturating_sub(1),
        NavDirection::Down | NavDirection::Right => (current_index + 1).min(order.len() - 1),
    };
    EvidenceNavOutcome::Focus(order[next_index])
}

pub fn activate_pane(
    pane: EvidencePane,
    input: EvidenceLensInput<'_>,
    _state: &FlightDeckState,
) -> EvidenceNavOutcome {
    match pane {
        EvidencePane::Search | EvidencePane::Timeline => input
            .proof_hits()
            .first()
            .map(|proof| {
                EvidenceNavOutcome::Intent(AppIntent::Navigate(AppRoute::Proof(
                    proof.proof_id.clone(),
                )))
            })
            .unwrap_or(EvidenceNavOutcome::None),
        EvidencePane::Graph => build_entity_proof_graph(input)
            .first_entity()
            .map(|entity| EvidenceNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(entity))))
            .unwrap_or(EvidenceNavOutcome::None),
        EvidencePane::Receipts => input
            .receipt_hits()
            .first()
            .map(|receipt| {
                EvidenceNavOutcome::Intent(AppIntent::ActionReceipt {
                    receipt_id: receipt.receipt_id.clone(),
                })
            })
            .unwrap_or(EvidenceNavOutcome::None),
        EvidencePane::Bundle => EvidenceNavOutcome::None,
    }
}
