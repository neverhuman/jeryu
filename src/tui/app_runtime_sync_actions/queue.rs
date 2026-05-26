//! Owner: Interactive TUI subsystem — queue/job/test actions (pools, pipelines, tests).
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Mutations are policy-gated by the presence of a backing store and the
//! GitLab client; UI state shrinks in lock-step with backend deletions.
use crate::tui::app::{ActivePane, App, TestViewMode};
use anyhow::Result;

impl App {
    pub async fn toggle_pool_paused(&mut self) -> Result<()> {
        let Some(store) = self.store.as_ref() else {
            return Ok(());
        };
        if let Some(pool) = self.state.pools.get(self.selected_pool_index) {
            if pool.paused {
                crate::pool::resume_pool(store, &self.gitlab, &pool.name).await?;
            } else {
                crate::pool::pause_pool(store, &self.gitlab, &pool.name).await?;
            }
        }
        Ok(())
    }

    pub async fn remove_selected_item(&mut self) -> Result<()> {
        let Some(store) = self.store.as_ref() else {
            return Ok(());
        };
        match self.active_pane {
            ActivePane::Pipelines => {
                if let Some(pm) = self.state.pipelines.get(self.selected_pipeline_index) {
                    let pid = pm.pipeline.pipeline_id;
                    store.delete_pipeline(pid).await?;
                    // Remove from local state immediately for snappy UX
                    self.state.pipelines.remove(self.selected_pipeline_index);
                    if self.selected_pipeline_index > 0 {
                        self.selected_pipeline_index -= 1;
                    }
                }
            }
            ActivePane::Jobs => {
                if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
                    let jid = j.job_id;
                    store.delete_job_event(jid).await?;
                    self.state.recent_jobs.remove(self.selected_job_index);
                    if self.selected_job_index > 0 {
                        self.selected_job_index -= 1;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn requeue_selected_job(&mut self) -> Result<()> {
        if self.active_pane == ActivePane::Jobs
            && let Some(j) = self.state.recent_jobs.get(self.selected_job_index)
            && j.status == "failed"
        {
            self.gitlab.requeue_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub fn toggle_test_view_mode(&mut self) {
        self.test_view_mode = match self.test_view_mode {
            TestViewMode::Average => TestViewMode::Latest,
            TestViewMode::Latest => TestViewMode::Average,
        };
        self.selected_test_index = 0;
        self.selected_test_history = None;
    }

    pub async fn fetch_selected_test_history(&mut self) {
        let Some(store) = self.store.as_ref() else {
            return;
        };
        let bottlenecks = match self.test_view_mode {
            TestViewMode::Average => &self.state.test_bottlenecks_avg,
            TestViewMode::Latest => &self.state.test_bottlenecks_latest,
        };
        if let Some(b) = bottlenecks.get(self.selected_test_index)
            && let Ok(hist) = store.get_test_history(&b.test_name, 50).await
        {
            self.selected_test_history = Some(hist);
        }
    }

    pub async fn cancel_selected_job(&mut self) -> Result<()> {
        if let Some(j) = self.state.recent_jobs.get(self.selected_job_index) {
            self.gitlab.cancel_job(j.project_id, j.job_id).await?;
        }
        Ok(())
    }

    pub async fn force_refresh(&mut self) {
        self.refresh_now().await;
    }
}
