use super::*;

#[cfg(test)]
pub(crate) async fn test_app() -> anyhow::Result<App> {
    let store = TuiSession::open_memory().await?;
    let docker = crate::tui::test_support::docker_ctl()?;
    let gitlab = GitlabClient::new("http://127.0.0.1:9", None);
    Ok(App::new(store, docker, gitlab))
}
impl App {
    pub fn new(store: TuiSession, docker: DockerCtl, gitlab: GitlabClient) -> Self {
        let (sync_tx, sync_rx) = mpsc::channel(4);
        let (flow_tx, flow_rx) = mpsc::channel(4);
        let (log_tx, log_rx) = mpsc::channel(8);
        let (feed_tx, feed_rx) = mpsc::channel(4);
        let (log_target_tx, _log_target_rx) = watch::channel(None);
        Self {
            store,
            docker,
            gitlab,
            state: TuiStateSnapshot::default(),
            active_tab: ActiveTab::default(),
            active_pane: ActivePane::default(),
            release_subpane: ReleaseSubPane::default(),
            selected_approval_index: 0,
            selected_pool_index: 0,
            selected_pipeline_index: 0,
            selected_job_index: 0,
            selected_job_id: None,
            maximize_logs: false,
            log_scroll_offset: 0,
            follow_log_tail: true,
            test_view_mode: TestViewMode::default(),
            selected_test_index: 0,
            selected_test_history: None,
            selected_evidence_index: 0,
            command_palette_open: false,
            command_palette_query: String::new(),
            selected_palette_index: 0,
            evidence_view_mode: EvidenceViewMode::default(),
            tick_count: 0,
            log_target: None,
            log_target_tx,
            feed_scroll_offset: 0,
            feed_follow_tail: true,
            feed_pinned: None,
            search_active: false,
            search_query: String::new(),
            help_overlay_open: false,
            workflow_nav: crate::tui::workflow::nav::WorkflowNav::default(),
            workflow_snapshot: crate::tui::workflow::model::WorkflowSnapshot::empty(),
            workflow_inspect_open: false,
            delivery_snapshot: crate::tui::workflow::model::DeliverySnapshot::empty(),
            inspector_tab: crate::tui::workflow::inspector::InspectorTab::default(),
            delivery_hit_map: crate::tui::workflow::hit_map::DeliveryHitMap::default(),
            drag_origin: None,
            sync_rx,
            sync_tx,
            log_rx,
            log_tx,
            flow_rx,
            flow_tx,
            feed_rx,
            feed_tx,
        }
    }

    pub fn apply_demo_fixture(&mut self) {
        app_runtime_demo::apply_demo_fixture(self);
    }

    pub fn tick_demo_state(&mut self) {
        app_runtime_demo::tick_demo_state(self);
    }
}

#[path = "app_runtime_demo.rs"]
mod app_runtime_demo;

#[path = "app_runtime_sync.rs"]
mod app_runtime_sync;
