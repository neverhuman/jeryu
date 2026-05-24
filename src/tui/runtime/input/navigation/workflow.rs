//! Owner: Interactive TUI subsystem - Workflow tab keybindings
//! Proof: `cargo nextest run -p jeryu -- tui::runtime::input::navigation::workflow`
//! Invariants: All shortcuts are no-ops outside `ActiveTab::Workflow`.
//!
//! Dispatched **before** `general::handle` so workflow-tab keys (`Enter`,
//! `r`, `f`, `b`, `c`, `z`, `Tab`/`BackTab` on the Inspector pane) win over
//! the global shortcuts defined there.

use crate::tui::{
    app::{ActiveTab, App},
    focus::PaneId,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) async fn handle(app: &mut App, key: KeyEvent) -> Result<Option<bool>> {
    if app.active_tab != ActiveTab::Workflow {
        return Ok(None);
    }
    match key.code {
        // Enter on the canvas opens (or closes) the right-side Inspector and
        // keeps drilled focus so arrow keys keep mutating node selection while
        // the Inspector is visible.
        KeyCode::Enter if app.focus.active == PaneId::WorkflowCanvas => {
            app.workflow_toggle_inspect();
            if app.workflow_inspect_open && !app.focus.is_drilled() {
                app.focus.push();
            } else if !app.workflow_inspect_open && app.focus.is_drilled() {
                let _ = app.focus.pop();
            }
            Ok(Some(false))
        }
        // Documented footer shortcuts. All actions already exist on `App`;
        // wiring them here is the entire fix.
        KeyCode::Char('r') => {
            app.workflow_trigger_rollback();
            Ok(Some(false))
        }
        KeyCode::Char('f') => {
            app.workflow_toggle_follow();
            Ok(Some(false))
        }
        KeyCode::Char('b') => {
            app.workflow_jump_to_blocker();
            Ok(Some(false))
        }
        KeyCode::Char('c') => {
            app.workflow_jump_to_critical_head();
            Ok(Some(false))
        }
        KeyCode::Char('z') => {
            app.workflow_cycle_zoom();
            Ok(Some(false))
        }
        // Tab cycles Inspector sub-tabs when the Inspector pane holds focus.
        KeyCode::Tab if app.focus.active == PaneId::WorkflowInspector => {
            app.inspector_cycle_next();
            Ok(Some(false))
        }
        KeyCode::BackTab if app.focus.active == PaneId::WorkflowInspector => {
            app.inspector_cycle_prev();
            Ok(Some(false))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::test_app;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    async fn workflow_app() -> Result<App> {
        let mut app = test_app().await?;
        app.apply_demo_fixture();
        app.active_tab = ActiveTab::Workflow;
        app.focus.set_tab(ActiveTab::Workflow);
        app.refresh_delivery_snapshot();
        app.focus.active = PaneId::WorkflowCanvas;
        Ok(app)
    }

    #[tokio::test]
    async fn enter_on_canvas_opens_inspector() -> Result<()> {
        let mut app = workflow_app().await?;
        app.workflow_inspect_open = false;
        assert_eq!(handle(&mut app, key(KeyCode::Enter)).await?, Some(false));
        assert!(app.workflow_inspect_open);
        assert!(app.focus.is_drilled());
        Ok(())
    }

    #[tokio::test]
    async fn enter_on_canvas_when_open_closes_inspector() -> Result<()> {
        let mut app = workflow_app().await?;
        app.workflow_inspect_open = true;
        app.focus.push();
        assert_eq!(handle(&mut app, key(KeyCode::Enter)).await?, Some(false));
        assert!(!app.workflow_inspect_open);
        assert!(!app.focus.is_drilled());
        Ok(())
    }

    #[tokio::test]
    async fn z_cycles_zoom() -> Result<()> {
        let mut app = workflow_app().await?;
        let before = app.workflow_nav.zoom;
        assert_eq!(handle(&mut app, key(KeyCode::Char('z'))).await?, Some(false));
        assert_ne!(app.workflow_nav.zoom, before);
        Ok(())
    }

    #[tokio::test]
    async fn f_toggles_follow() -> Result<()> {
        let mut app = workflow_app().await?;
        let before = app.workflow_nav.follow_active;
        assert_eq!(handle(&mut app, key(KeyCode::Char('f'))).await?, Some(false));
        assert_ne!(app.workflow_nav.follow_active, before);
        Ok(())
    }

    #[tokio::test]
    async fn r_triggers_rollback_message() -> Result<()> {
        let mut app = workflow_app().await?;
        assert_eq!(handle(&mut app, key(KeyCode::Char('r'))).await?, Some(false));
        assert!(app.delivery_action_message.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn b_jumps_to_blocker_no_panic() -> Result<()> {
        let mut app = workflow_app().await?;
        assert_eq!(handle(&mut app, key(KeyCode::Char('b'))).await?, Some(false));
        Ok(())
    }

    #[tokio::test]
    async fn c_jumps_to_critical_head_no_panic() -> Result<()> {
        let mut app = workflow_app().await?;
        assert_eq!(handle(&mut app, key(KeyCode::Char('c'))).await?, Some(false));
        Ok(())
    }

    #[tokio::test]
    async fn tab_on_inspector_cycles_subtab() -> Result<()> {
        let mut app = workflow_app().await?;
        app.focus.active = PaneId::WorkflowInspector;
        let before = app.inspector_tab;
        assert_eq!(handle(&mut app, key(KeyCode::Tab)).await?, Some(false));
        assert_ne!(app.inspector_tab, before);
        Ok(())
    }

    #[tokio::test]
    async fn outside_workflow_tab_is_noop() -> Result<()> {
        let mut app = test_app().await?;
        app.active_tab = ActiveTab::Bugs;
        app.focus.set_tab(ActiveTab::Bugs);
        assert_eq!(handle(&mut app, key(KeyCode::Char('b'))).await?, None);
        Ok(())
    }
}
