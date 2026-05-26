//! Owner: Interactive TUI subsystem — focus, log target tracking, and overlay helpers.
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Focus stack is consistent with overlay/fullscreen flags; the log
//! target watch channel always mirrors the currently visible job (or `None`).
use crate::tui::app::{ActiveTab, App, JobEvent, LogTarget};

impl App {
    pub(crate) fn update_log_target(&mut self) {
        if (self.maximize_logs || self.focus.fullscreen.is_some())
            && let Some(job) = self.selected_job()
        {
            let target = Some(LogTarget {
                project_id: job.project_id,
                job_id: job.job_id,
            });
            if self.log_target != target {
                self.log_target = target;
                let _ = self.log_target_tx.send(target);
            }
            return;
        }
        if self.log_target.is_some() {
            self.log_target = None;
            let _ = self.log_target_tx.send(None);
        }
    }

    pub(crate) fn sync_selected_job_index(&mut self) {
        if self.state.recent_jobs.is_empty() {
            self.selected_job_index = 0;
            self.selected_job_id = None;
            return;
        }

        if let Some(job_id) = self.selected_job_id
            && let Some(index) = self
                .state
                .recent_jobs
                .iter()
                .position(|job| job.job_id == job_id)
        {
            self.selected_job_index = index;
            return;
        }

        if self.selected_job_index >= self.state.recent_jobs.len() {
            self.selected_job_index = self.state.recent_jobs.len() - 1;
        }
        self.remember_selected_job();
    }

    pub(crate) fn remember_selected_job(&mut self) {
        self.selected_job_id = self.selected_job().map(|job| job.job_id);
    }

    pub fn selected_job(&self) -> Option<&JobEvent> {
        self.state.recent_jobs.get(self.selected_job_index)
    }

    pub fn open_selected_job_log(&mut self) {
        self.active_pane = crate::tui::app::ActivePane::Jobs;
        self.remember_selected_job();
        self.maximize_logs = true;
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
        self.update_log_target();
    }

    pub fn close_log_view(&mut self) {
        self.maximize_logs = false;
        self.focus.fullscreen = None;
        self.update_log_target();
    }

    pub fn open_activity_log(&mut self) {
        let pane = crate::tui::focus::PaneId::ActivityLog(self.active_tab);
        self.focus.push();
        self.focus.active = pane;
        self.focus.fullscreen = Some(pane);
        self.maximize_logs = true;
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
    }

    pub fn close_focus_overlay(&mut self) -> bool {
        if self.command_palette_open {
            self.command_palette_open = false;
            self.command_palette_query.clear();
            self.selected_palette_index = 0;
            return true;
        }
        if self.help_overlay_open {
            self.help_overlay_open = false;
            return true;
        }
        if self.maximize_logs || self.focus.fullscreen.is_some() || !self.focus.stack.is_empty() {
            self.maximize_logs = false;
            self.focus.fullscreen = None;
            if self.focus.pop() {
                self.update_log_target();
            }
            return true;
        }
        false
    }

    pub fn enter_focused_pane(&mut self) {
        let pane = self.focus.active;
        if matches!(pane, crate::tui::focus::PaneId::ActivityLog(_)) {
            self.open_activity_log();
            return;
        }
        self.focus.push();
        match (self.active_tab, pane) {
            (ActiveTab::Release, crate::tui::focus::PaneId::ReleaseSelector) => {
                self.release_subpane = self.release_subpane.next();
            }
            (ActiveTab::Jobs, crate::tui::focus::PaneId::JobsRunnerFeed) => {
                self.feed_toggle_pin();
            }
            (ActiveTab::Tests, crate::tui::focus::PaneId::TestsBottlenecks) => {
                self.selected_test_history = None;
            }
            _ => {}
        }
    }

    pub fn current_focus_pane(&self) -> crate::tui::focus::PaneId {
        self.focus.active
    }

    pub fn focus_move(&mut self, direction: crate::tui::focus::NavDirection) {
        if self.focus.is_drilled() {
            return;
        }
        if let Some(next) = self.focus_map.neighbor(self.focus.active, direction) {
            self.maximize_logs = false;
            self.focus.active = next;
        } else if self.focus_map.rect_of(self.focus.active).is_none()
            && let Some(first) = self.focus_map.first_visible()
        {
            self.maximize_logs = false;
            self.focus.active = first;
        }
    }

    pub fn scroll_logs_up(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_logs_down(&mut self, amount: u16) {
        self.follow_log_tail = false;
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(amount);
    }

    pub fn follow_logs(&mut self) {
        self.follow_log_tail = true;
        self.log_scroll_offset = u16::MAX;
    }

    pub fn jump_logs_top(&mut self) {
        self.follow_log_tail = false;
        self.log_scroll_offset = 0;
    }
}
