use crate::tui::{
    app::{
        reducer::AppIntent,
        state::{AppRoute, FlightDeckState},
    },
    lenses::mission::MissionLensInput,
    nav::NavDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionPane {
    Posture,
    TopBlocker,
    Freshness,
    NextAction,
    ProofLinks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionNavOutcome {
    Focus(MissionPane),
    Intent(AppIntent),
    None,
}

pub fn move_focus(current: MissionPane, direction: NavDirection) -> MissionNavOutcome {
    let order = [
        MissionPane::Posture,
        MissionPane::TopBlocker,
        MissionPane::Freshness,
        MissionPane::NextAction,
        MissionPane::ProofLinks,
    ];
    let current_index = order
        .iter()
        .position(|pane| *pane == current)
        .unwrap_or_default();
    let next_index = match direction {
        NavDirection::Up | NavDirection::Left => current_index.saturating_sub(1),
        NavDirection::Down | NavDirection::Right => (current_index + 1).min(order.len() - 1),
    };
    MissionNavOutcome::Focus(order[next_index])
}

pub fn activate_pane(
    pane: MissionPane,
    input: MissionLensInput<'_>,
    _state: &FlightDeckState,
) -> MissionNavOutcome {
    match pane {
        MissionPane::TopBlocker => input
            .top_blocker()
            .and_then(|blocker| blocker.entity.clone())
            .map(|entity| MissionNavOutcome::Intent(AppIntent::SelectEntity(Some(entity))))
            .unwrap_or(MissionNavOutcome::None),
        MissionPane::NextAction => input
            .next_action()
            .map(|action| {
                MissionNavOutcome::Intent(AppIntent::BeginActionPreview {
                    action_id: action.action_ref.action_id.clone(),
                })
            })
            .unwrap_or(MissionNavOutcome::None),
        MissionPane::ProofLinks => input
            .proof_links()
            .first()
            .map(|proof| {
                MissionNavOutcome::Intent(AppIntent::Navigate(AppRoute::Proof((*proof).into())))
            })
            .unwrap_or(MissionNavOutcome::None),
        MissionPane::Posture | MissionPane::Freshness => MissionNavOutcome::None,
    }
}
