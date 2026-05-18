//! Owner: Interactive TUI subsystem - runtime rendering helpers
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Rendering helpers stay pure over the provided application snapshot.

mod png;

#[cfg(test)]
mod tests;

use anyhow::Result;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

pub(crate) use png::write_buffer_png;

pub(crate) fn cleanup_screenshot_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    terminal.show_cursor()?;
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    Ok(())
}

/// Render one deterministic TUI frame into a PNG file.
pub(crate) fn parse_capture_tab(tab: &str) -> Result<crate::tui::app::ActiveTab> {
    match tab.to_ascii_lowercase().as_str() {
        "workflow" | "0" => Ok(crate::tui::app::ActiveTab::Workflow),
        "mission" => Ok(crate::tui::app::ActiveTab::Mission),
        "release" => Ok(crate::tui::app::ActiveTab::Release),
        "approvals" => Ok(crate::tui::app::ActiveTab::Approvals),
        "jobs" | "flow" => Ok(crate::tui::app::ActiveTab::Jobs),
        "agents" => Ok(crate::tui::app::ActiveTab::Agents),
        "tests" | "vti" => Ok(crate::tui::app::ActiveTab::Tests),
        "pools" => Ok(crate::tui::app::ActiveTab::Pools),
        "cache" => Ok(crate::tui::app::ActiveTab::Cache),
        "evidence" | "audit" => Ok(crate::tui::app::ActiveTab::Evidence),
        "llms" | "llm" => Ok(crate::tui::app::ActiveTab::LLMs),
        "secrets" => Ok(crate::tui::app::ActiveTab::Secrets),
        "git" => Ok(crate::tui::app::ActiveTab::Git),
        _ => anyhow::bail!(
            "unknown TUI tab '{}'; expected workflow, mission, release, approvals, jobs, agents, tests, pools, cache, evidence, llms, secrets, or git",
            tab
        ),
    }
}
