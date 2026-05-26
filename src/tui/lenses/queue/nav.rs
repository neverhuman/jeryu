use crate::tui::{
    app::{
        reducer::AppIntent,
        state::{AppRoute, FlightDeckState},
    },
    lenses::queue::QueueLensInput,
    nav::NavDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePane {
    Capacity,
    Lab,
    Jobs,
    Pools,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueNavOutcome {
    Focus(QueuePane),
    Intent(AppIntent),
    None,
}

pub fn move_focus(current: QueuePane, direction: NavDirection) -> QueueNavOutcome {
    let order = [
        QueuePane::Capacity,
        QueuePane::Lab,
        QueuePane::Jobs,
        QueuePane::Pools,
    ];
    let current_index = order
        .iter()
        .position(|pane| *pane == current)
        .unwrap_or_default();
    let next_index = match direction {
        NavDirection::Up | NavDirection::Left => current_index.saturating_sub(1),
        NavDirection::Down | NavDirection::Right => (current_index + 1).min(order.len() - 1),
    };
    QueueNavOutcome::Focus(order[next_index])
}

pub fn activate_pane(
    pane: QueuePane,
    input: QueueLensInput<'_>,
    _state: &FlightDeckState,
) -> QueueNavOutcome {
    match pane {
        QueuePane::Jobs => input
            .waiting_jobs()
            .first()
            .map(|job| {
                QueueNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(job.entity.clone())))
            })
            .unwrap_or(QueueNavOutcome::None),
        QueuePane::Pools => input
            .first_pool_entity()
            .map(|entity| QueueNavOutcome::Intent(AppIntent::SelectEntity(Some(entity))))
            .unwrap_or(QueueNavOutcome::None),
        QueuePane::Capacity | QueuePane::Lab => QueueNavOutcome::None,
    }
}
