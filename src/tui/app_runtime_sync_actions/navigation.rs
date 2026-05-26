//! Owner: Interactive TUI subsystem — vertical row navigation in selection panes.
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Selection wraps within the active pane's row count; log target is
//! resynchronised after each move.
use crate::tui::app::{ActivePane, ActiveTab, App, TestViewMode};

impl App {
    pub fn up(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                if self.selected_test_index > 0 {
                    self.selected_test_index -= 1;
                } else {
                    self.selected_test_index = limit - 1;
                }
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    if self.selected_pool_index > 0 {
                        self.selected_pool_index -= 1;
                    } else {
                        self.selected_pool_index = self.state.pools.len() - 1;
                    }
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    } else {
                        self.selected_pipeline_index = self.state.pipelines.len() - 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    } else {
                        self.selected_job_index = self.state.recent_jobs.len() - 1;
                    }
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }

    pub fn down(&mut self) {
        if self.active_tab == ActiveTab::Tests {
            let limit = match self.test_view_mode {
                TestViewMode::Average => self.state.test_bottlenecks_avg.len(),
                TestViewMode::Latest => self.state.test_bottlenecks_latest.len(),
            };
            if limit > 0 {
                self.selected_test_index = (self.selected_test_index + 1) % limit;
                self.selected_test_history = None; // clear history when moving
            }
            return;
        }

        match self.active_pane {
            ActivePane::Pools => {
                if !self.state.pools.is_empty() {
                    self.selected_pool_index =
                        (self.selected_pool_index + 1) % self.state.pools.len();
                }
            }
            ActivePane::Pipelines => {
                if !self.state.pipelines.is_empty() {
                    self.selected_pipeline_index =
                        (self.selected_pipeline_index + 1) % self.state.pipelines.len();
                }
            }
            ActivePane::Jobs => {
                if !self.state.recent_jobs.is_empty() {
                    self.selected_job_index =
                        (self.selected_job_index + 1) % self.state.recent_jobs.len();
                    self.remember_selected_job();
                }
            }
        }
        self.update_log_target();
    }
}
