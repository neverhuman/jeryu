use crate::tui::{
    app::{
        reducer::AppIntent,
        state::{AppRoute, FlightDeckState},
    },
    lenses::repos::ReposLensInput,
    nav::NavDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReposPane {
    Fleet,
    Families,
    Repos,
    Detail,
    Attention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReposNavOutcome {
    Focus(ReposPane),
    Intent(AppIntent),
    None,
}

pub fn move_focus(current: ReposPane, direction: NavDirection) -> ReposNavOutcome {
    let order = [
        ReposPane::Fleet,
        ReposPane::Families,
        ReposPane::Repos,
        ReposPane::Detail,
        ReposPane::Attention,
    ];
    let current_index = order
        .iter()
        .position(|pane| *pane == current)
        .unwrap_or_default();
    let next_index = match direction {
        NavDirection::Up | NavDirection::Left => current_index.saturating_sub(1),
        NavDirection::Down | NavDirection::Right => (current_index + 1).min(order.len() - 1),
    };
    ReposNavOutcome::Focus(order[next_index])
}

pub fn activate_pane(
    pane: ReposPane,
    input: ReposLensInput<'_>,
    _state: &FlightDeckState,
) -> ReposNavOutcome {
    match pane {
        ReposPane::Families => input
            .selected_family()
            .map(|family| {
                ReposNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(family.entity)))
            })
            .unwrap_or(ReposNavOutcome::None),
        ReposPane::Repos | ReposPane::Detail => input
            .selected_repo()
            .map(|repo| {
                ReposNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(repo.entity.clone())))
            })
            .unwrap_or(ReposNavOutcome::None),
        ReposPane::Attention => input
            .scoped_attention()
            .first()
            .map(|item| {
                ReposNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(item.entity.clone())))
            })
            .unwrap_or(ReposNavOutcome::None),
        ReposPane::Fleet => ReposNavOutcome::None,
    }
}
