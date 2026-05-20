use crate::tui::{
    app::{ActiveTab, App},
    focus::{NavDirection, PaneId},
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) async fn handle(app: &mut App, key: KeyEvent) -> Result<Option<bool>> {
    match key.code {
        KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => {
            app.command_palette_open = true;
            app.command_palette_query.clear();
            app.selected_palette_index = 0;
            Ok(Some(false))
        }
        KeyCode::Char('q') => Ok(Some(true)),
        KeyCode::Esc => {
            let _ = app.close_focus_overlay();
            Ok(Some(false))
        }
        KeyCode::Char('?') => {
            app.help_overlay_open = !app.help_overlay_open;
            Ok(Some(false))
        }
        KeyCode::Char('b') => {
            app.active_tab = ActiveTab::Bugs;
            app.maximize_logs = false;
            app.focus.set_tab(ActiveTab::Bugs);
            Ok(Some(false))
        }
        KeyCode::F(5) => {
            app.force_refresh().await;
            Ok(Some(false))
        }
        KeyCode::Char('p') => {
            app.toggle_pool_paused().await?;
            Ok(Some(false))
        }
        KeyCode::Tab => {
            app.cycle_tab_next();
            Ok(Some(false))
        }
        KeyCode::BackTab => {
            app.cycle_tab_prev();
            Ok(Some(false))
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let Some(tab) = ActiveTab::from_number(c.to_digit(10).unwrap() as u8) {
                app.active_tab = tab;
                app.maximize_logs = false;
                app.focus.set_tab(tab);
            }
            Ok(Some(false))
        }
        KeyCode::Enter => {
            handle_enter(app).await;
            Ok(Some(false))
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if app.focus.is_drilled() {
                handle_drilled_arrow(app, NavDirection::Left).await;
            } else {
                app.focus_move(NavDirection::Left);
            }
            Ok(Some(false))
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if app.focus.is_drilled() {
                handle_drilled_arrow(app, NavDirection::Right).await;
            } else {
                app.focus_move(NavDirection::Right);
            }
            Ok(Some(false))
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if is_activity_fullscreen(app) {
                app.scroll_logs_up(1);
            } else if app.focus.is_drilled() {
                handle_drilled_arrow(app, NavDirection::Up).await;
            } else {
                app.focus_move(NavDirection::Up);
            }
            Ok(Some(false))
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if is_activity_fullscreen(app) {
                app.scroll_logs_down(1);
            } else if app.focus.is_drilled() {
                handle_drilled_arrow(app, NavDirection::Down).await;
            } else {
                app.focus_move(NavDirection::Down);
            }
            Ok(Some(false))
        }
        KeyCode::PageUp => {
            if is_activity_fullscreen(app) {
                app.scroll_logs_up(20);
                Ok(Some(false))
            } else {
                Ok(None)
            }
        }
        KeyCode::PageDown | KeyCode::Char(' ') => {
            if is_activity_fullscreen(app) {
                app.scroll_logs_down(20);
                Ok(Some(false))
            } else {
                Ok(None)
            }
        }
        KeyCode::Home => {
            if is_activity_fullscreen(app) {
                app.jump_logs_top();
                Ok(Some(false))
            } else {
                Ok(None)
            }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if is_activity_fullscreen(app) {
                app.follow_logs();
                Ok(Some(false))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

async fn handle_enter(app: &mut App) {
    let pane = app.focus.active;
    match (app.active_tab, pane) {
        (_, PaneId::ActivityLog(_)) if app.focus.fullscreen.is_none() => {
            app.open_activity_log();
        }
        (_, PaneId::ActivityLog(_)) => {}
        (ActiveTab::Tests, PaneId::TestsBottlenecks) => {
            app.focus.push();
            app.fetch_selected_test_history().await;
        }
        (ActiveTab::Release, PaneId::ReleaseSelector) => {
            app.focus.push();
            app.release_subpane = app.release_subpane.next();
        }
        (ActiveTab::Jobs, PaneId::JobsRunnerFeed) => {
            app.focus.push();
            app.feed_toggle_pin();
        }
        _ => {
            app.focus.push();
        }
    }
}

async fn handle_drilled_arrow(app: &mut App, direction: NavDirection) {
    match (app.active_tab, app.focus.active, direction) {
        (ActiveTab::Workflow, PaneId::WorkflowPrRail, NavDirection::Left) => {
            app.delivery_prev_pr();
        }
        (ActiveTab::Workflow, PaneId::WorkflowPrRail, NavDirection::Right) => {
            app.delivery_next_pr();
        }
        (ActiveTab::Workflow, PaneId::WorkflowPhaseRail, NavDirection::Up) => {
            app.workflow_up();
        }
        (ActiveTab::Workflow, PaneId::WorkflowPhaseRail, NavDirection::Down) => {
            app.workflow_down();
        }
        (ActiveTab::Workflow, PaneId::WorkflowCanvas, NavDirection::Left) => {
            app.workflow_left();
        }
        (ActiveTab::Workflow, PaneId::WorkflowCanvas, NavDirection::Right) => {
            app.workflow_right();
        }
        (ActiveTab::Workflow, PaneId::WorkflowCanvas, NavDirection::Up) => {
            app.workflow_up();
        }
        (ActiveTab::Workflow, PaneId::WorkflowCanvas, NavDirection::Down) => {
            app.workflow_down();
        }
        (ActiveTab::Release, PaneId::ReleaseSelector, NavDirection::Left) => {
            app.release_subpane = app.release_subpane.prev();
        }
        (ActiveTab::Release, PaneId::ReleaseSelector, NavDirection::Right) => {
            app.release_subpane = app.release_subpane.next();
        }
        (ActiveTab::Approvals, PaneId::ApprovalsQueue, NavDirection::Up) => {
            app.selected_approval_index = app.selected_approval_index.saturating_sub(1);
        }
        (ActiveTab::Approvals, PaneId::ApprovalsQueue, NavDirection::Down) => {
            let len = app.state.approvals_queue.len();
            if len > 0 {
                app.selected_approval_index = (app.selected_approval_index + 1).min(len - 1);
            }
        }
        (ActiveTab::Jobs, PaneId::JobsRunnerFeed, NavDirection::Left) => {
            app.feed_prev();
        }
        (ActiveTab::Jobs, PaneId::JobsRunnerFeed, NavDirection::Right) => {
            app.feed_next();
        }
        (ActiveTab::Jobs, PaneId::JobsMatrix | PaneId::JobsInspector, NavDirection::Up) => {
            if !app.state.recent_jobs.is_empty() {
                app.selected_job_index = app.selected_job_index.saturating_sub(1);
                app.remember_selected_job();
            }
        }
        (ActiveTab::Jobs, PaneId::JobsMatrix | PaneId::JobsInspector, NavDirection::Down) => {
            if !app.state.recent_jobs.is_empty() {
                app.selected_job_index = (app.selected_job_index + 1) % app.state.recent_jobs.len();
                app.remember_selected_job();
            }
        }
        (ActiveTab::Agents, PaneId::AgentsSessions, NavDirection::Up) => {
            if !app.state.agent_pipelines.is_empty() {
                app.selected_job_index = app.selected_job_index.saturating_sub(1);
            }
        }
        (ActiveTab::Agents, PaneId::AgentsSessions, NavDirection::Down) => {
            if !app.state.agent_pipelines.is_empty() {
                app.selected_job_index =
                    (app.selected_job_index + 1) % app.state.agent_pipelines.len();
            }
        }
        (ActiveTab::Tests, PaneId::TestsBottlenecks, NavDirection::Up) => {
            let len = match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => app.state.test_bottlenecks_avg.len(),
                crate::tui::app::TestViewMode::Latest => app.state.test_bottlenecks_latest.len(),
            };
            if len > 0 {
                app.selected_test_index = app.selected_test_index.saturating_sub(1);
            }
        }
        (ActiveTab::Tests, PaneId::TestsBottlenecks, NavDirection::Down) => {
            let len = match app.test_view_mode {
                crate::tui::app::TestViewMode::Average => app.state.test_bottlenecks_avg.len(),
                crate::tui::app::TestViewMode::Latest => app.state.test_bottlenecks_latest.len(),
            };
            if len > 0 {
                app.selected_test_index = (app.selected_test_index + 1).min(len - 1);
            }
        }
        (ActiveTab::Pools, PaneId::PoolsList, NavDirection::Up) => {
            app.selected_pool_index = app.selected_pool_index.saturating_sub(1);
        }
        (ActiveTab::Pools, PaneId::PoolsList, NavDirection::Down) => {
            if !app.state.pools.is_empty() {
                app.selected_pool_index = (app.selected_pool_index + 1) % app.state.pools.len();
            }
        }
        (ActiveTab::Evidence, PaneId::EvidenceList, NavDirection::Up) => {
            app.selected_evidence_index = app.selected_evidence_index.saturating_sub(1);
        }
        (ActiveTab::Evidence, PaneId::EvidenceList, NavDirection::Down) => {
            if !app.state.recent_evidence.is_empty() {
                app.selected_evidence_index =
                    (app.selected_evidence_index + 1) % app.state.recent_evidence.len();
            }
        }
        (ActiveTab::Secrets, PaneId::SecretsList, NavDirection::Up) => {
            app.selected_secret_index = app.selected_secret_index.saturating_sub(1);
        }
        (ActiveTab::Secrets, PaneId::SecretsList, NavDirection::Down) => {
            if !app.state.secret_audit_events.is_empty() {
                app.selected_secret_index =
                    (app.selected_secret_index + 1) % app.state.secret_audit_events.len();
            }
        }
        (ActiveTab::Git, PaneId::GitLedger, NavDirection::Up) => {
            app.selected_git_index = app.selected_git_index.saturating_sub(1);
        }
        (ActiveTab::Git, PaneId::GitLedger, NavDirection::Down) => {
            if !app.state.recent_git_events.is_empty() {
                app.selected_git_index =
                    (app.selected_git_index + 1) % app.state.recent_git_events.len();
            }
        }
        _ => {}
    }
}

fn is_activity_fullscreen(app: &App) -> bool {
    app.maximize_logs || app.focus.fullscreen == Some(PaneId::ActivityLog(app.active_tab))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::test_app;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[tokio::test]
    async fn tab_and_backtab_cycle_tabs() -> Result<()> {
        let mut app = test_app().await?;
        assert_eq!(app.active_tab, ActiveTab::Workflow);

        assert_eq!(handle(&mut app, key(KeyCode::Tab)).await?, Some(false));
        assert_eq!(app.active_tab, ActiveTab::Mission);

        app.active_tab = ActiveTab::Workflow;
        app.focus.set_tab(ActiveTab::Workflow);
        assert_eq!(handle(&mut app, key(KeyCode::BackTab)).await?, Some(false));
        assert_eq!(app.active_tab, ActiveTab::Git);
        Ok(())
    }

    #[tokio::test]
    async fn esc_is_non_quitting_at_root() -> Result<()> {
        let mut app = test_app().await?;
        assert_eq!(handle(&mut app, key(KeyCode::Esc)).await?, Some(false));
        Ok(())
    }

    #[tokio::test]
    async fn enter_and_escape_restore_activity_log_focus() -> Result<()> {
        let mut app = test_app().await?;
        app.active_tab = ActiveTab::Jobs;
        app.focus.set_tab(ActiveTab::Jobs);
        app.focus.active = PaneId::ActivityLog(ActiveTab::Jobs);

        assert_eq!(handle(&mut app, key(KeyCode::Enter)).await?, Some(false));
        assert_eq!(
            app.focus.fullscreen,
            Some(PaneId::ActivityLog(ActiveTab::Jobs))
        );

        assert_eq!(handle(&mut app, key(KeyCode::Esc)).await?, Some(false));
        assert_eq!(app.focus.fullscreen, None);
        Ok(())
    }
}
