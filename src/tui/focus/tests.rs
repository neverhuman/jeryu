use crate::tui::{
    app::ActiveTab,
    focus::{FocusMap, FocusState, NavDirection, PaneId},
};
use ratatui::layout::Rect;

#[test]
fn tab_change_resets_focus_stack_and_fullscreen() {
    let mut focus = FocusState::for_tab(ActiveTab::Workflow);
    focus.active = PaneId::WorkflowCanvas;
    focus.push();
    focus.fullscreen = Some(PaneId::ActivityLog(ActiveTab::Workflow));

    focus.set_tab(ActiveTab::Jobs);

    assert_eq!(focus.active, PaneId::JobsRunnerFeed);
    assert!(focus.stack.is_empty());
    assert_eq!(focus.fullscreen, None);
}

#[test]
fn escape_restores_previous_focus_after_fullscreen() {
    let mut focus = FocusState::for_tab(ActiveTab::Workflow);
    focus.active = PaneId::WorkflowCanvas;
    focus.push();
    focus.active = PaneId::ActivityLog(ActiveTab::Workflow);
    focus.fullscreen = Some(PaneId::ActivityLog(ActiveTab::Workflow));

    assert!(focus.escape());

    assert_eq!(focus.fullscreen, None);
    assert_eq!(focus.active, PaneId::WorkflowCanvas);
    assert!(!focus.is_drilled());
}

#[test]
fn arrow_neighbors_follow_visible_geometry() {
    let mut map = FocusMap::default();
    map.clear_for_tab(ActiveTab::Workflow);
    map.register(PaneId::WorkflowPrRail, Rect::new(0, 0, 20, 20));
    map.register(PaneId::WorkflowCanvas, Rect::new(21, 0, 60, 20));
    map.register(PaneId::WorkflowInspector, Rect::new(82, 0, 20, 20));
    map.register(
        PaneId::ActivityLog(ActiveTab::Workflow),
        Rect::new(21, 21, 60, 10),
    );

    assert_eq!(
        map.neighbor(PaneId::WorkflowPrRail, NavDirection::Right),
        Some(PaneId::WorkflowCanvas)
    );
    assert_eq!(
        map.neighbor(PaneId::WorkflowCanvas, NavDirection::Down),
        Some(PaneId::ActivityLog(ActiveTab::Workflow))
    );
}

#[test]
fn pane_lists_keep_activity_logs_tab_scoped() {
    assert_eq!(
        PaneId::default_for_tab(ActiveTab::Mission),
        PaneId::MissionTopSignal
    );
    assert!(
        PaneId::panes_for_tab(ActiveTab::Mission)
            .contains(&PaneId::ActivityLog(ActiveTab::Mission))
    );
    assert_eq!(
        PaneId::ActivityLog(ActiveTab::Cache).tab(),
        ActiveTab::Cache
    );
}
