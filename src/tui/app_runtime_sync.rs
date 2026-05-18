use super::*;
#[path = "app_runtime_sync_background.rs"]
mod background;
impl App {
    pub fn start_background_sync(&self) {
        background::start_background_sync(self);
    }
    pub async fn refresh_now(&mut self) {
        let mut snap = TuiStateSnapshot::default();
        if let Some(store) = self.store.as_ref() {
            Self::hydrate_core_snapshot(&mut snap, store, &self.docker, &self.gitlab).await;
        }
        self.state = snap;
    }

    pub async fn hydrate_release_status(&mut self) {
        if let Some(store) = self.store.as_ref()
            && let Ok(report) = crate::release::build_release_status_report(
                store,
                crate::release::ReleaseStatusQuery {
                    project_id: Some(crate::release::DEFAULT_RELEASE_PROJECT_ID),
                    ref_name: Some("main".to_string()),
                    sha: None,
                    limit: 1,
                },
            )
            .await
        {
            self.state.release_status_generated_at = Some(report.generated_at);
            self.state.release_status = report.latest;
        }
    }

    async fn hydrate_core_snapshot(
        snap: &mut TuiStateSnapshot,
        store: &TuiSession,
        docker: &DockerCtl,
        gitlab: &GitlabClient,
    ) {
        if let Ok(pools) = store.list_pools().await {
            snap.pools = pools;
        }

        if let Ok(managed) = docker.list_managed_containers().await {
            snap.active_containers = managed.len();
        }

        let mut jobs = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        if let Ok(active_jobs) = gitlab
            .list_jobs(
                release::DEFAULT_RELEASE_PROJECT_ID,
                &[
                    "running",
                    "pending",
                    "created",
                    "waiting_for_resource",
                    "preparing",
                ],
            )
            .await
        {
            let now = chrono::Utc::now().to_rfc3339();
            for job in active_jobs {
                seen.insert(job.id);
                let pipeline_id = job.effective_pipeline_id();
                let pool_name = Some(
                    match job.runner.as_ref().and_then(|r| r.description.as_deref()) {
                        Some(desc) => desc.to_owned(),
                        None => job.stage.clone(),
                    },
                );
                jobs.push(JobEvent {
                    job_id: job.id,
                    project_id: release::DEFAULT_RELEASE_PROJECT_ID,
                    pipeline_id,
                    status: job.status,
                    job_name: Some(job.name),
                    pool_name,
                    system_id: None,
                    queued_duration: job.queued_duration,
                    received_at: match job.started_at {
                        Some(value) => value,
                        None => now.clone(),
                    },
                });
            }
        }

        if let Ok(recent_jobs_from_store) = store.recent_job_events(50).await {
            jobs.extend(
                recent_jobs_from_store
                    .into_iter()
                    .filter(|job| seen.insert(job.job_id))
                    .take(50usize.saturating_sub(jobs.len())),
            );
        }

        jobs.sort_by(|left, right| {
            crate::tui::live::live_job_status_rank(&right.status)
                .cmp(&crate::tui::live::live_job_status_rank(&left.status))
                .then_with(|| right.received_at.cmp(&left.received_at))
                .then_with(|| right.job_id.cmp(&left.job_id))
        });
        snap.recent_jobs = jobs;
        snap.gitlab_ready = gitlab.is_ready().await;
    }

    pub async fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        while let Ok(mut state) = self.sync_rx.try_recv() {
            // Preserve live sub-state that is updated on separate channels
            state.flow = self.state.flow.clone();
            state.live_log = self.state.live_log.clone();
            state.inspector_capsule = self.state.inspector_capsule.clone();
            state.inspector_job_id = self.state.inspector_job_id;
            // Preserve TUI v2 feed state
            state.runner_feeds = self.state.runner_feeds.clone();
            state.active_feed_index = self.state.active_feed_index;
            state.feed_cycle_tick = self.state.feed_cycle_tick;
            state.feed_auto_cycle = self.state.feed_auto_cycle;
            // Only preserve demo/manually-set view when background sync found nothing
            if state.pipeline_progress_view.is_none() {
                state.pipeline_progress_view = self.state.pipeline_progress_view.clone();
            }
            state.event_ticker_offset = self.state.event_ticker_offset;
            self.state = state;
        }

        while let Ok(flow_snap) = self.flow_rx.try_recv() {
            self.apply_flow_snapshot(flow_snap);
        }

        while let Ok(log_state) = self.log_rx.try_recv() {
            self.state.live_log = log_state;
        }

        // TUI v2 — consume runner feed updates
        while let Ok(feeds) = self.feed_rx.try_recv() {
            self.state.runner_feeds = feeds;
        }

        // TUI v2 — auto-cycle runner feed every FEED_CYCLE_TICKS (5s at 250ms tick)
        if !self.state.runner_feeds.is_empty() && self.feed_pinned.is_none() {
            self.state.feed_cycle_tick = self.state.feed_cycle_tick.wrapping_add(1);
            if self.state.feed_cycle_tick.is_multiple_of(FEED_CYCLE_TICKS) {
                self.state.active_feed_index =
                    (self.state.active_feed_index + 1) % self.state.runner_feeds.len();
                self.state.feed_auto_cycle = true;
                self.feed_scroll_offset = 0;
                self.feed_follow_tail = true;
            }
        }
        if let Some(pinned) = self.feed_pinned {
            self.state.active_feed_index =
                pinned.min(self.state.runner_feeds.len().saturating_sub(1));
            self.state.feed_auto_cycle = false;
        }

        // TUI v2 — advance event ticker
        if self.tick_count.is_multiple_of(2) {
            self.state.event_ticker_offset = self.state.event_ticker_offset.wrapping_add(1);
        }

        // Clamp indices
        if self.selected_pool_index >= self.state.pools.len() && !self.state.pools.is_empty() {
            self.selected_pool_index = self.state.pools.len() - 1;
        }
        if self.selected_pipeline_index >= self.state.pipelines.len()
            && !self.state.pipelines.is_empty()
        {
            self.selected_pipeline_index = self.state.pipelines.len() - 1;
        }
        self.sync_selected_job_index();
        self.update_log_target();

        // Fetch inspector capsule when selected job changes
        let current_job_id = self.selected_job_id;
        if current_job_id != self.state.inspector_job_id {
            self.state.inspector_job_id = current_job_id;
            if let Some(jid) = current_job_id {
                self.state.inspector_capsule = if let Some(store) = self.store.as_ref() {
                    store.latest_evidence_by_job_id(jid).await.ok().flatten()
                } else {
                    None
                };
            } else {
                self.state.inspector_capsule = None;
            }
        }
    }

    fn apply_flow_snapshot(&mut self, mut flow_snap: crate::tui::flow::FlowSnapshot) {
        if flow_snap.active_pipelines.is_empty() && !self.state.flow.active_pipelines.is_empty() {
            flow_snap.active_pipelines = self.state.flow.active_pipelines.clone();
            flow_snap.outdated = true;
            flow_snap.last_non_empty_at = self
                .state
                .flow
                .last_non_empty_at
                .or(Some(self.state.flow.generated_at));
            flow_snap.selected_pipeline_id = self.state.flow.selected_pipeline_id;
        } else if flow_snap.active_pipelines.is_empty()
            && let Some(recovery) = self.flow_from_recent_jobs(flow_snap.generated_at)
        {
            flow_snap.active_pipelines = vec![recovery];
            flow_snap.outdated = true;
            flow_snap.last_non_empty_at = Some(flow_snap.generated_at);
        } else if !flow_snap.active_pipelines.is_empty() {
            flow_snap.last_non_empty_at =
                flow_snap.last_non_empty_at.or(Some(flow_snap.generated_at));
        }

        self.state.flow = flow_snap;
    }

    fn flow_from_recent_jobs(
        &self,
        _generated_at: chrono::DateTime<chrono::Utc>,
    ) -> Option<crate::tui::flow::PipelineFlow> {
        let release = self.state.release_status.as_ref();
        let selected_pipeline = self.state.pipelines.get(self.selected_pipeline_index);
        let pipeline_id = if let Some(view) = release {
            if let Some(id) = view.attempt.release_pipeline_id {
                Some(id)
            } else if let Some(metrics) = selected_pipeline {
                Some(metrics.pipeline.pipeline_id)
            } else {
                self.state
                    .recent_jobs
                    .iter()
                    .find_map(|job| job.pipeline_id)
            }
        } else if let Some(metrics) = selected_pipeline {
            Some(metrics.pipeline.pipeline_id)
        } else {
            self.state
                .recent_jobs
                .iter()
                .find_map(|job| job.pipeline_id)
        };

        let project_id = if let Some(view) = release {
            view.attempt.project_id
        } else {
            self.state
                .recent_jobs
                .first()
                .map(|job| job.project_id)
                .unwrap_or(release::DEFAULT_RELEASE_PROJECT_ID)
        };

        if pipeline_id.is_none() {
            return crate::tui::flow::recover_flow_from_recent_jobs(
                project_id,
                &self.state.recent_jobs,
            );
        }

        let pipeline_id = pipeline_id?;
        let jobs = self
            .state
            .recent_jobs
            .iter()
            .filter(|job| job.pipeline_id == Some(pipeline_id))
            .cloned()
            .collect::<Vec<_>>();

        if jobs.is_empty() {
            return crate::tui::flow::recover_flow_from_recent_jobs(
                project_id,
                &self.state.recent_jobs,
            );
        }

        let ref_name = if let Some(view) = release {
            view.attempt.ref_name.clone()
        } else {
            match self
                .state
                .pipelines
                .iter()
                .find(|m| m.pipeline.pipeline_id == pipeline_id)
            {
                Some(m) => m.pipeline.ref_name.clone(),
                None => "main".to_string(),
            }
        };

        let sha = if let Some(view) = release {
            Some(view.attempt.sha.clone())
        } else {
            self.state
                .pipelines
                .iter()
                .find(|metrics| metrics.pipeline.pipeline_id == pipeline_id)
                .map(|metrics| metrics.pipeline.sha.clone())
        };

        let status = if let Some(view) = release {
            match view.attempt.release_pipeline_status.clone() {
                Some(s) => s,
                None => "unknown".to_string(),
            }
        } else {
            match self
                .state
                .pipelines
                .iter()
                .find(|m| m.pipeline.pipeline_id == pipeline_id)
            {
                Some(m) => m.pipeline.status.clone(),
                None => "unknown".to_string(),
            }
        };

        Some(crate::tui::flow::pipeline_flow_from_jobs(
            pipeline_id,
            project_id,
            ref_name,
            sha,
            status,
            jobs,
        ))
    }
}

#[path = "app_runtime_sync_actions.rs"]
mod actions;

#[cfg(test)]
#[path = "app_runtime_sync_tests.rs"]
mod tests;

fn retain_tail(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }

    let mut start = input.len().saturating_sub(max_bytes);
    while !input.is_char_boundary(start) {
        start += 1;
    }

    format!("... (truncated)\n{}", &input[start..])
}

/// Recursively calculate the size of a directory in bytes.
async fn dir_size_bytes(path: &std::path::Path) -> i64 {
    let mut total: i64 = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if meta.is_file() {
                    total += meta.len() as i64;
                } else if meta.is_dir() {
                    total += Box::pin(dir_size_bytes(&entry.path())).await;
                }
            }
        }
    }
    total
}

pub(crate) fn build_stage_progress_from_ci_runs(
    runs: &[crate::state::CiJobRun],
) -> Vec<StageProgress> {
    use std::collections::HashMap;
    let mut stage_order: Vec<String> = Vec::new();
    let mut stage_map: HashMap<String, StageProgress> = HashMap::new();

    for run in runs {
        if !stage_map.contains_key(&run.stage) {
            stage_order.push(run.stage.clone());
            stage_map.insert(
                run.stage.clone(),
                StageProgress {
                    stage_name: run.stage.clone(),
                    ..Default::default()
                },
            );
        }
        let entry = stage_map.get_mut(&run.stage).unwrap();
        entry.total_jobs += 1;
        match run.status.as_str() {
            "success" => entry.completed_jobs += 1,
            "running" => entry.running_jobs += 1,
            "failed" | "canceled" => entry.failed_jobs += 1,
            _ => {}
        }
    }

    stage_order
        .into_iter()
        .map(|name| {
            let mut s = stage_map.remove(&name).unwrap();
            s.status = stage_status_str(&s);
            s
        })
        .collect()
}

pub(crate) fn build_stage_progress_from_events(
    events: &[crate::state::JobEvent],
    pipeline_id: i64,
) -> Vec<StageProgress> {
    use std::collections::HashMap;
    let mut stage_order: Vec<String> = Vec::new();
    let mut stage_map: HashMap<String, StageProgress> = HashMap::new();

    for event in events.iter().filter(|e| e.pipeline_id == Some(pipeline_id)) {
        let stage = event
            .pool_name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        if !stage_map.contains_key(&stage) {
            stage_order.push(stage.clone());
            stage_map.insert(
                stage.clone(),
                StageProgress {
                    stage_name: stage.clone(),
                    ..Default::default()
                },
            );
        }
        let entry = stage_map.get_mut(&stage).unwrap();
        entry.total_jobs += 1;
        match event.status.as_str() {
            "success" => entry.completed_jobs += 1,
            "running" => entry.running_jobs += 1,
            "failed" | "canceled" => entry.failed_jobs += 1,
            _ => {}
        }
    }

    stage_order
        .into_iter()
        .map(|name| {
            let mut s = stage_map.remove(&name).unwrap();
            s.status = stage_status_str(&s);
            s
        })
        .collect()
}

fn stage_status_str(s: &StageProgress) -> String {
    if s.failed_jobs > 0 {
        "failed".into()
    } else if s.running_jobs > 0 {
        "running".into()
    } else if s.completed_jobs == s.total_jobs && s.total_jobs > 0 {
        "success".into()
    } else {
        "pending".into()
    }
}
