use crate::tui::{action_registry, app::App};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_palette_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.command_palette_open = false;
            app.command_palette_query.clear();
            app.selected_palette_index = 0;
        }
        KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE => {
            app.command_palette_query.push(c);
            app.selected_palette_index = 0;
        }
        KeyCode::Backspace => {
            app.command_palette_query.pop();
            app.selected_palette_index = 0;
        }
        KeyCode::Up => {
            app.selected_palette_index = app.selected_palette_index.saturating_sub(1);
        }
        KeyCode::Down => {
            let count = action_registry::filtered(&app.command_palette_query).count();
            if count > 0 {
                app.selected_palette_index = (app.selected_palette_index + 1).min(count - 1);
            }
        }
        KeyCode::Enter => {
            execute_palette_action(app);
            app.command_palette_open = false;
            app.command_palette_query.clear();
            app.selected_palette_index = 0;
        }
        _ => {}
    }
}

fn execute_palette_action(app: &mut App) {
    let matches: Vec<&action_registry::ActionEntry> =
        action_registry::filtered(&app.command_palette_query).collect();
    let Some(entry) = matches.get(app.selected_palette_index) else {
        return;
    };
    match entry.id {
        "tab_mission" => app.active_tab = crate::tui::app::ActiveTab::Mission,
        "tab_release" => app.active_tab = crate::tui::app::ActiveTab::Release,
        "tab_jobs" => app.active_tab = crate::tui::app::ActiveTab::Jobs,
        "tab_agents" => app.active_tab = crate::tui::app::ActiveTab::Agents,
        "tab_tests" => app.active_tab = crate::tui::app::ActiveTab::Tests,
        "tab_pools" => app.active_tab = crate::tui::app::ActiveTab::Pools,
        "tab_cache" => app.active_tab = crate::tui::app::ActiveTab::Cache,
        "tab_evidence" => app.active_tab = crate::tui::app::ActiveTab::Evidence,
        "tab_secrets" => app.active_tab = crate::tui::app::ActiveTab::Secrets,
        "tab_git" => app.active_tab = crate::tui::app::ActiveTab::Git,
        "toggle_audit_ledger" => {
            app.evidence_view_mode = match app.evidence_view_mode {
                crate::tui::app::EvidenceViewMode::Capsules => {
                    crate::tui::app::EvidenceViewMode::AuditLedger
                }
                crate::tui::app::EvidenceViewMode::AuditLedger => {
                    crate::tui::app::EvidenceViewMode::Capsules
                }
            };
        }
        _ => {}
    }
}
