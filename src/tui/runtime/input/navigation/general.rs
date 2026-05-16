use crate::tui::app::{ActiveTab, App};
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
            if app.workflow_inspect_open {
                app.workflow_inspect_open = false;
                Ok(Some(false))
            } else if app.maximize_logs {
                app.close_log_view();
                Ok(Some(false))
            } else {
                Ok(Some(true))
            }
        }
        KeyCode::Char('?') => {
            app.help_overlay_open = !app.help_overlay_open;
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

        // ─── Workflow-specific keys ────────────────────────────────
        // Arrow keys, Tab, Enter, panning — intercepted when Workflow tab is active.
        KeyCode::Up | KeyCode::Char('k')
            if app.active_tab == ActiveTab::Workflow && !app.maximize_logs =>
        {
            app.workflow_up();
            Ok(Some(false))
        }
        KeyCode::Down | KeyCode::Char('j')
            if app.active_tab == ActiveTab::Workflow && !app.maximize_logs =>
        {
            app.workflow_down();
            Ok(Some(false))
        }
        KeyCode::Left | KeyCode::Char('h')
            if app.active_tab == ActiveTab::Workflow && !app.maximize_logs =>
        {
            app.workflow_left();
            Ok(Some(false))
        }
        KeyCode::Right | KeyCode::Char('l')
            if app.active_tab == ActiveTab::Workflow && !app.maximize_logs =>
        {
            app.workflow_right();
            Ok(Some(false))
        }
        KeyCode::Tab if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            if app.workflow_inspect_open {
                app.inspector_cycle_next();
            } else {
                app.workflow_tab_next();
            }
            Ok(Some(false))
        }
        KeyCode::BackTab if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            if app.workflow_inspect_open {
                app.inspector_cycle_prev();
            }
            Ok(Some(false))
        }
        KeyCode::Enter if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_toggle_inspect();
            Ok(Some(false))
        }
        KeyCode::PageDown | KeyCode::Char(' ')
            if app.active_tab == ActiveTab::Workflow && !app.maximize_logs =>
        {
            app.workflow_page_down();
            Ok(Some(false))
        }
        KeyCode::PageUp if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_page_up();
            Ok(Some(false))
        }
        KeyCode::Char(']') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_page_right();
            Ok(Some(false))
        }
        KeyCode::Char('[') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_page_left();
            Ok(Some(false))
        }
        KeyCode::Home if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_home();
            Ok(Some(false))
        }
        KeyCode::End if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_end();
            Ok(Some(false))
        }
        KeyCode::Char('f') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_toggle_follow();
            Ok(Some(false))
        }
        KeyCode::Char('b') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_jump_to_blocker();
            Ok(Some(false))
        }
        KeyCode::Char('c') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.workflow_jump_to_critical_head();
            Ok(Some(false))
        }
        // Cycle pull requests: '<' previous, '>' next (or shift+arrows-style intent).
        KeyCode::Char('<') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.delivery_prev_pr();
            Ok(Some(false))
        }
        KeyCode::Char('>') if app.active_tab == ActiveTab::Workflow && !app.maximize_logs => {
            app.delivery_next_pr();
            Ok(Some(false))
        }

        // ─── General keys (non-workflow) ───────────────────────────
        KeyCode::Tab if app.active_tab != ActiveTab::Workflow => {
            app.cycle_tab_next();
            Ok(Some(false))
        }
        KeyCode::Right if app.active_tab != ActiveTab::Workflow => {
            app.cycle_pane_next();
            Ok(Some(false))
        }
        KeyCode::Left if app.active_tab != ActiveTab::Workflow => {
            app.cycle_pane_prev();
            Ok(Some(false))
        }
        KeyCode::Up if app.active_tab != ActiveTab::Workflow => {
            if app.maximize_logs {
                app.scroll_logs_up(1);
            } else {
                app.up();
            }
            Ok(Some(false))
        }
        KeyCode::Down if app.active_tab != ActiveTab::Workflow => {
            if app.maximize_logs {
                app.scroll_logs_down(1);
            } else {
                app.down();
            }
            Ok(Some(false))
        }
        KeyCode::PageUp if app.maximize_logs => {
            app.scroll_logs_up(20);
            Ok(Some(false))
        }
        KeyCode::PageDown | KeyCode::Char(' ') if app.maximize_logs => {
            app.scroll_logs_down(20);
            Ok(Some(false))
        }
        KeyCode::Char('G') | KeyCode::End if app.maximize_logs => {
            app.follow_logs();
            Ok(Some(false))
        }
        KeyCode::Home if app.maximize_logs => {
            app.jump_logs_top();
            Ok(Some(false))
        }
        _ => Ok(None),
    }
}
