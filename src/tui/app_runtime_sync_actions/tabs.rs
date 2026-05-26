//! Owner: Interactive TUI subsystem — tab and pane cycling.
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Tab cycling clears the maximize-logs flag and resynchronises focus.
use crate::tui::app::{ActiveTab, App};

impl App {
    pub fn cycle_tab_next(&mut self) {
        self.maximize_logs = false;
        self.active_tab = match self.active_tab {
            ActiveTab::Workflow => ActiveTab::Mission,
            ActiveTab::Mission => ActiveTab::Release,
            ActiveTab::Release => ActiveTab::Approvals,
            ActiveTab::Approvals => ActiveTab::Jobs,
            ActiveTab::Jobs => ActiveTab::Agents,
            ActiveTab::Agents => ActiveTab::Tests,
            ActiveTab::Tests => ActiveTab::Pools,
            ActiveTab::Pools => ActiveTab::Cache,
            ActiveTab::Cache => ActiveTab::Evidence,
            ActiveTab::Evidence => ActiveTab::Bugs,
            ActiveTab::Bugs => ActiveTab::Secrets,
            ActiveTab::Secrets => ActiveTab::LLMs,
            ActiveTab::LLMs => ActiveTab::Git,
            ActiveTab::Git => ActiveTab::Workflow,
        };
        self.focus.set_tab(self.active_tab);
    }

    pub fn cycle_tab_prev(&mut self) {
        self.maximize_logs = false;
        self.active_tab = match self.active_tab {
            ActiveTab::Workflow => ActiveTab::Git,
            ActiveTab::Mission => ActiveTab::Workflow,
            ActiveTab::Release => ActiveTab::Mission,
            ActiveTab::Approvals => ActiveTab::Release,
            ActiveTab::Jobs => ActiveTab::Approvals,
            ActiveTab::Agents => ActiveTab::Jobs,
            ActiveTab::Tests => ActiveTab::Agents,
            ActiveTab::Pools => ActiveTab::Tests,
            ActiveTab::Cache => ActiveTab::Pools,
            ActiveTab::Evidence => ActiveTab::Cache,
            ActiveTab::LLMs => ActiveTab::Secrets,
            ActiveTab::Git => ActiveTab::LLMs,
            ActiveTab::Secrets => ActiveTab::Bugs,
            ActiveTab::Bugs => ActiveTab::Evidence,
        };
        self.focus.set_tab(self.active_tab);
    }

    pub fn cycle_pane_next(&mut self) {
        if let Some(next) = self
            .focus_map
            .neighbor(self.focus.active, crate::tui::focus::NavDirection::Right)
        {
            self.focus.active = next;
        }
    }

    pub fn cycle_pane_prev(&mut self) {
        if let Some(prev) = self
            .focus_map
            .neighbor(self.focus.active, crate::tui::focus::NavDirection::Left)
        {
            self.focus.active = prev;
        }
    }
}
