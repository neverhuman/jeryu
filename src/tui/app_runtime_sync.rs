use super::*;
use crate::tui::workflow::live_delivery::{DeliveryHosts, LiveDeliveryCollector};
#[path = "app_runtime_sync_background.rs"]
mod background;
#[path = "app_runtime_sync_progress.rs"]
mod progress;
use progress::*;

impl App {
    pub fn start_background_sync(&self) {
        background::start_background_sync(self);
    }
    pub async fn refresh_now(&mut self) {
        let mut snap = TuiStateSnapshot::default();
        if let Some(store) = self.store.as_ref() {
            Self::hydrate_core_snapshot(
                &mut snap,
                store,
                &self.docker,
                &self.gitlab,
                &self.state.pools,
            )
            .await;
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

    pub async fn hydrate_delivery_status(&mut self) {
        let Some(_) = self.store.as_ref() else {
            self.state.delivery_source_status = crate::tui::app::DeliverySourceStatus {
                configured: false,
                backend_label: None,
                source_label: None,
                last_sync_at: Some(chrono::Utc::now()),
                last_sync_error: None,
            };
            return;
        };

        let workspace_root = crate::repo_fleet::resolve_workspace_root().await;
        let hosts = DeliveryHosts::from_env();
        let mut collector = LiveDeliveryCollector::new();
        let update = collector
            .sync(
                workspace_root.as_deref(),
                &hosts,
                self.state.release_status.as_ref(),
            )
            .await;
        self.set_delivery_snapshot(update.snapshot, update.source_status);
    }

    async fn hydrate_core_snapshot(
        snap: &mut TuiStateSnapshot,
        store: &TuiSession,
        docker: &DockerCtl,
        gitlab: &GitlabClient,
        last_good_pools: &[Pool],
    ) {
        merge_pool_sync_result(
            &mut snap.pools,
            &mut snap.pool_sync_error,
            last_good_pools,
            store.list_pools().await,
        );

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
        if jobs.len() < 50
            && let Ok(recent_runs) = store.recent_ci_job_runs(50).await
        {
            jobs.extend(
                recent_runs
                    .into_iter()
                    .map(ci_job_run_to_recent_job)
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

        // Populate remote node summaries from local config + DB.
        snap.remote_nodes = build_node_summaries(store).await;

        if let Some(repo_root) = crate::repo_fleet::resolve_workspace_root().await {
            match crate::repo_fleet::collect_fleet_snapshot(&repo_root, None).await {
                Ok(fleet) => {
                    snap.delivery_source_status = crate::tui::app::DeliverySourceStatus {
                        configured: true,
                        backend_label: Some("GitHub".into()),
                        source_label: Some(format!("fleet registry: {}", fleet.registry_path)),
                        last_sync_at: Some(chrono::Utc::now()),
                        last_sync_error: None,
                    };
                    snap.fleet = fleet;
                }
                Err(e) => {
                    tracing::warn!("fleet hydration failed for {}: {e:#}", repo_root.display())
                }
            }
        }
        if snap.fleet.repos.is_empty()
            && let Ok(repos) = store.list_tracked_repositories().await
            && !repos.is_empty()
            && let Ok(fleet) =
                crate::repo_fleet::collect_fleet_snapshot_from_tracked_repositories(&repos, None)
                    .await
        {
            snap.delivery_source_status = crate::tui::app::DeliverySourceStatus {
                configured: true,
                backend_label: Some("GitHub".into()),
                source_label: Some("tracked repositories".into()),
                last_sync_at: Some(chrono::Utc::now()),
                last_sync_error: None,
            };
            snap.fleet = fleet;
        }
    }

    pub async fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        while let Ok(mut state) = self.sync_rx.try_recv() {
            // Preserve live sub-state that is updated on separate channels
            state.flow = self.state.flow.clone();
            if state.fleet.repos.is_empty() {
                state.fleet = self.state.fleet.clone();
            }
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

        while let Ok(update) = self.delivery_rx.try_recv() {
            self.set_delivery_snapshot(update.snapshot, update.source_status);
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
        if self.selected_repo_index > self.state.fleet.repos.len() {
            self.selected_repo_index = self.state.fleet.repos.len();
        }
        self.clamp_bug_selection();
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

    pub(crate) fn clamp_bug_selection(&mut self) {
        self.selected_bug_index =
            crate::tui::bugs::clamp_bug_index(self.selected_bug_index, self.state.bugs.len());
        let project_count = self
            .state
            .bugs
            .iter()
            .map(|bug| &bug.target_project)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        self.selected_bug_project_index =
            crate::tui::bugs::clamp_bug_index(self.selected_bug_project_index, project_count);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoolSyncMerge {
    Fresh,
    CachedFallback,
}

pub(crate) fn merge_pool_sync_result(
    snapshot_pools: &mut Vec<Pool>,
    pool_sync_error: &mut Option<String>,
    last_good_pools: &[Pool],
    result: Result<Vec<Pool>>,
) -> PoolSyncMerge {
    match result {
        Ok(pools) => {
            *snapshot_pools = pools;
            *pool_sync_error = None;
            PoolSyncMerge::Fresh
        }
        Err(error) => {
            *snapshot_pools = last_good_pools.to_vec();
            *pool_sync_error = Some(compact_pool_sync_error(error));
            PoolSyncMerge::CachedFallback
        }
    }
}

/// Build `NodeSummary` entries from local node configs + DB manager counts.
/// This is a cheap, filesystem+DB-only operation (no SSH probing on every tick).
async fn build_node_summaries(store: &TuiSession) -> Vec<crate::tui::app::NodeSummary> {
    let configs = match crate::node_support::list_node_configs() {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    if configs.is_empty() {
        return vec![];
    }

    let mut summaries = Vec::with_capacity(configs.len());

    for cfg in &configs {
        // Count active managers on this node from the DB.
        let active_managers = match store.list_managers_for_node(&cfg.alias).await {
            Ok(managers) => managers
                .iter()
                .filter(|m| {
                    matches!(
                        m.state.as_str(),
                        "starting" | "online" | "node_starting" | "node_unreachable" | "draining"
                    )
                })
                .count(),
            Err(_) => 0,
        };

        // Most-recent last_contact_at across all managers on this node.
        let last_contact_at = match store.list_managers_for_node(&cfg.alias).await {
            Ok(managers) => managers
                .iter()
                .filter_map(|m| m.last_contact_at.clone())
                .max(),
            Err(_) => None,
        };

        summaries.push(crate::tui::app::NodeSummary {
            alias: cfg.alias.clone(),
            target: cfg.target.clone(),
            enabled: cfg.enabled,
            reachable: None, // populated by doctor/probe, not every tick
            active_managers,
            max_managers: cfg.max_managers,
            storage_used_gb: None, // populated asynchronously by a separate probe
            storage_limit_gb: cfg.storage_limit_gb,
            last_contact_at,
        });
    }

    summaries
}

fn compact_pool_sync_error(error: anyhow::Error) -> String {
    const MAX_DETAIL_CHARS: usize = 96;
    let detail = format!("{error:#}")
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("unavailable")
        .to_string();
    let mut chars = detail.chars();
    let short = chars.by_ref().take(MAX_DETAIL_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("pool sync failed: {short}...")
    } else {
        format!("pool sync failed: {short}")
    }
}

fn ci_job_run_to_recent_job(run: crate::state::CiJobRun) -> JobEvent {
    JobEvent {
        job_id: run.job_id,
        project_id: run.project_id,
        pipeline_id: Some(run.pipeline_id),
        status: run.status,
        job_name: Some(run.job_name),
        pool_name: run.runner_pool.or(run.runner),
        system_id: None,
        queued_duration: run.queued_duration_secs,
        received_at: run
            .finished_at
            .or(run.started_at)
            .unwrap_or(run.observed_at),
    }
}

#[path = "app_runtime_sync_actions/mod.rs"]
mod actions;

#[cfg(test)]
#[path = "app_runtime_sync_tests.rs"]
mod tests;
