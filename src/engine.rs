//! Owner: Engine Core (Webhook + Reconciliation)
//! Proof: `cargo test -p jeryu -- engine`
//! Invariants: 5-min recon cycle; Docker crash recovery via event stream; supersedence on newer SHA
//!
//! The engine is the real-time brain. It runs two concurrent tasks:
//! 1. An Axum HTTP server that receives GitLab webhook events
//! 2. A periodic reconciliation loop that syncs desired vs actual state

use anyhow::Result;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::decision::{RetryDecision as RecoveryDecision, SupersedenceAction, SupersedenceDecision};
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::impact;
use crate::pool;
use crate::release;
use crate::state::{Db, JobEvent, TrackedPipeline};

#[path = "engine_aux.rs"]
mod aux_secondary;

// ---------------------------------------------------------------------------
// Shared state for the engine
// ---------------------------------------------------------------------------

pub struct EngineState {
    pub db: Db,
    pub docker: DockerCtl,
    pub client: GitlabClient,
    pub webhook_secret: String,
}

pub type SharedState = Arc<EngineState>;

// ---------------------------------------------------------------------------
// Webhook payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JobHookPayload {
    build_id: Option<i64>,
    project_id: Option<i64>,
    pipeline_id: Option<i64>,
    build_status: Option<String>,
    build_name: Option<String>,
    build_queued_duration: Option<f64>,
    tag: Option<bool>,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
    runner: Option<RunnerInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RunnerInfo {
    id: Option<i64>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PipelineHookPayload {
    project: Option<ProjectInfo>,
    object_attributes: Option<PipelineAttributes>,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct PipelineAttributes {
    id: Option<i64>,
    status: Option<String>,
    sha: Option<String>,
    #[serde(rename = "ref")]
    ref_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PushHookPayload {
    project_id: i64,
    before: String,
    after: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn health() -> &'static str {
    "ok"
}

async fn handle_webhook(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    // Verify webhook secret
    let token = headers
        .get("X-Gitlab-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if token != state.webhook_secret {
        warn!("webhook rejected: invalid token");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let event_type = headers
        .get("X-Gitlab-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    debug!(event_type, "received webhook");

    match event_type {
        "Job Hook" => {
            if let Ok(payload) = serde_json::from_str::<JobHookPayload>(&body) {
                handle_job_event(&state, payload).await;
            } else {
                warn!("failed to parse Job Hook payload");
            }
        }
        "Pipeline Hook" => {
            if let Ok(payload) = serde_json::from_str::<PipelineHookPayload>(&body) {
                handle_pipeline_event(state.clone(), payload).await;
            } else {
                warn!("failed to parse Pipeline Hook payload");
            }
        }
        "Push Hook" => {
            if let Ok(payload) = serde_json::from_str::<PushHookPayload>(&body) {
                // Do semantic evaluation
                tokio::spawn(handle_push_event(state.clone(), payload));
            } else {
                warn!("failed to parse Push Hook payload");
            }
        }
        "Merge Request Hook" => {
            debug!("merge request event received (logged, not acted on yet)");
        }
        _ => {
            debug!(event_type, "unhandled webhook event type");
        }
    }

    Ok(StatusCode::OK)
}

async fn handle_push_event(state: SharedState, payload: PushHookPayload) {
    let ref_name = normalize_ref(&payload.ref_name);
    info!(
        project_id = payload.project_id,
        ref_name = %ref_name,
        before = %payload.before,
        after = %payload.after,
        "intercepted Push Hook for Semantic CI Evaluation"
    );

    // Skip supersedence and impact analysis for ephemeral test branches.
    // These branches experience out-of-order push hooks (branch-create hook
    // can arrive after the commit hook) which causes the supersedence logic
    // to incorrectly cancel the test pipeline.
    if ref_name.starts_with("jeryu-test-") {
        debug!(
            project_id = payload.project_id,
            ref_name = %ref_name,
            "skipping supersedence/impact for ephemeral test branch"
        );
        return;
    }

    // For demonstration, we simply log the semantic diff hook activation.
    if crate::decision::is_branch_creation_push(&payload.before) {
        debug!(
            project_id = payload.project_id,
            "semantic evaluation bypassed: branch creation event"
        );
        return;
    }

    if let Err(e) = handle_supersedence(&state, payload.project_id, &ref_name, &payload.after).await
    {
        error!(error = %e, project_id = payload.project_id, ref_name = %ref_name, "supersedence evaluation failed");
    }

    match impact::plan_for_push(
        &state.client,
        payload.project_id,
        &payload.before,
        &payload.after,
    )
    .await
    {
        Ok(plan) => {
            let payload_json = impact::render_plan_payload(&plan);
            if let Err(e) = state
                .db
                .append_event(
                    "impact_decision",
                    Some(payload.project_id),
                    None,
                    "engine",
                    &payload_json.to_string(),
                )
                .await
            {
                error!(error = %e, "failed to persist impact decision");
            }

            info!(
                project_id = payload.project_id,
                ref_name = %ref_name,
                lanes = ?plan.selected_lanes,
                recovery_path = plan.widened_to_full,
                "semantic CI impact plan computed"
            );

            // VTI: Record test plan for later auditing if changed paths are available
            if !plan.affected_paths.is_empty() {
                let vti_plan = crate::test_intel::planner::plan_tests(&plan.affected_paths);
                let vti_json = crate::test_intel::explain::explain_json(&vti_plan);
                let mode = format!("{:?}", vti_plan.mode);
                let subsystems = vti_plan.affected_subsystems.join(",");
                if let Err(e) = state
                    .db
                    .record_test_plan(
                        payload.project_id,
                        &payload.before,
                        &payload.after,
                        &mode,
                        vti_plan.confidence,
                        vti_plan.selected_tests.len() as i64,
                        vti_plan.skipped_subsystems.len() as i64,
                        &subsystems,
                        vti_plan.repair_reason(),
                        &vti_json.to_string(),
                    )
                    .await
                {
                    error!(error = %e, "failed to persist VTI test plan");
                } else {
                    info!(
                        project_id = payload.project_id,
                        mode = %mode,
                        confidence = vti_plan.confidence,
                        selected = vti_plan.selected_tests.len(),
                        skipped = vti_plan.skipped_subsystems.len(),
                        "VTI test plan recorded"
                    );
                }
            }
        }
        Err(e) => {
            error!(error = %e, project_id = payload.project_id, ref_name = %ref_name, "impact analysis failed");
        }
    }
}

async fn handle_job_event(state: &EngineState, payload: JobHookPayload) {
    let Some(job_id) = payload.build_id else {
        return;
    };
    let Some(project_id) = payload.project_id else {
        return;
    };
    let status = match payload.build_status {
        Some(s) => s,
        None => String::new(),
    };

    info!(
        job_id,
        project_id,
        status = %status,
        "job event"
    );

    // Record the event
    let event = JobEvent {
        job_id,
        project_id,
        pipeline_id: payload.pipeline_id,
        status: status.clone(),
        job_name: payload.build_name,
        pool_name: None, // resolved during reconciliation
        system_id: None,
        queued_duration: payload.build_queued_duration,
        received_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = state.db.upsert_job_event(&event).await {
        error!(error = %e, "failed to record job event");
    }

    if status == "failed"
        && let Err(e) = maybe_secondary_attempt_failed_job(state, project_id, job_id).await
    {
        error!(error = %e, project_id, job_id, "secondary attempt decision failed");
    }

    // If a job is pending, check if we need to scale up
    if (status == "pending" || status == "created")
        && let Err(e) = check_scale_up(state).await
    {
        error!(error = %e, "scale-up check failed");
    }
}

async fn handle_pipeline_event(state: SharedState, payload: PipelineHookPayload) {
    if let Some(attrs) = payload.object_attributes {
        info!(
            pipeline_id = attrs.id,
            status = attrs.status,
            ref_name = attrs.ref_name,
            "pipeline event"
        );

        if let (Some(pipeline_id), Some(status), Some(ref_name), Some(sha)) =
            (attrs.id, attrs.status, attrs.ref_name, attrs.sha)
        {
            let ref_name = normalize_ref(&ref_name);
            let project_id = match payload.project.and_then(|project| project.id) {
                Some(id) => id,
                None => 0,
            };
            let _ = state
                .db
                .upsert_tracked_pipeline(&TrackedPipeline {
                    pipeline_id,
                    project_id,
                    ref_name: ref_name.clone(),
                    sha: sha.clone(),
                    status: status.clone(),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                })
                .await;

            if ref_name == "main" && status == "success" {
                if let Ok(Some(attempt)) = state
                    .db
                    .release_attempt_by_production_pipeline_id(pipeline_id)
                    .await
                {
                    if let Err(err) = state
                        .db
                        .update_production_pipeline_status(pipeline_id, &status)
                        .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "failed to refresh production pipeline status"
                        );
                    } else {
                        info!(
                            project_id,
                            pipeline_id,
                            sha = %attempt.sha,
                            version = %attempt.version,
                            "production-promotion pipeline passed"
                        );
                    }
                    return;
                }

                if let Ok(Some(attempt)) = state
                    .db
                    .release_attempt_by_release_pipeline_id(pipeline_id)
                    .await
                {
                    if let Err(err) = state
                        .db
                        .update_release_pipeline_status(pipeline_id, &status)
                        .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "failed to refresh release pipeline status"
                        );
                    } else {
                        info!(
                            project_id,
                            pipeline_id,
                            sha = %attempt.sha,
                            version = %attempt.version,
                            "release-execution pipeline passed"
                        );
                    }
                    let state = state.clone();
                    let ref_name = ref_name.clone();
                    tokio::spawn(async move {
                        if let Err(err) = release::maybe_trigger_production_promotion(
                            &state.db,
                            &state.client,
                            project_id,
                            &ref_name,
                            Some(&attempt.sha),
                            Some(&attempt.version),
                        )
                        .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                sha = %attempt.sha,
                                version = %attempt.version,
                                error = %err,
                                "automatic production promotion check failed"
                            );
                        }
                    });
                    return;
                }

                let state = state.clone();
                let ref_name = ref_name.clone();
                let sha = sha.clone();
                tokio::spawn(async move {
                    if let Err(err) = release::launch_canary_for_green_pipeline(
                        &state.db,
                        &state.client,
                        project_id,
                        &ref_name,
                        &sha,
                        pipeline_id,
                    )
                    .await
                    {
                        error!(
                            project_id,
                            pipeline_id,
                            sha = %sha,
                            error = %err,
                            "automatic canary launch failed"
                        );
                    }
                });
            } else if ref_name == "main"
                && matches!(status.as_str(), "failed" | "canceled" | "skipped")
            {
                match state
                    .db
                    .release_attempt_by_release_pipeline_id(pipeline_id)
                    .await
                {
                    Ok(Some(attempt)) => {
                        if let Err(err) = state
                            .db
                            .update_release_pipeline_status(pipeline_id, &status)
                            .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                status = %status,
                                error = %err,
                                "failed to refresh failed release-execution pipeline status"
                            );
                        } else {
                            let note = format!(
                                "release-execution pipeline {pipeline_id} ended with status {status}"
                            );
                            let _ = state
                                .db
                                .finish_release_canary(
                                    project_id,
                                    &ref_name,
                                    &attempt.sha,
                                    "failed",
                                    Some(&note),
                                )
                                .await;
                        }
                    }
                    Ok(None) => {
                        if let Ok(Some(_attempt)) = state
                            .db
                            .release_attempt_by_production_pipeline_id(pipeline_id)
                            .await
                            && let Err(err) = state
                                .db
                                .update_production_pipeline_status(pipeline_id, &status)
                                .await
                        {
                            error!(
                                project_id,
                                pipeline_id,
                                status = %status,
                                error = %err,
                                "failed to refresh failed production-promotion pipeline status"
                            );
                        }
                    }
                    Err(err) => {
                        debug!(
                            project_id,
                            pipeline_id,
                            status = %status,
                            error = %err,
                            "pipeline was not a tracked release pipeline"
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Scale-up logic
// ---------------------------------------------------------------------------

async fn check_scale_up(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    for p in &pools {
        if p.paused {
            continue;
        }

        let active = state.db.count_active_managers(&p.name).await?;
        let target = desired_manager_target(p, queued, running);

        if active < target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                min_warm = p.min_warm,
                "scaling up to meet queue demand"
            );
            pool::scale_pool_to(
                &state.db,
                &state.docker,
                &state.client,
                &p.name,
                target as usize,
            )
            .await?;
        }
    }

    Ok(())
}

async fn handle_supersedence(
    state: &EngineState,
    project_id: i64,
    ref_name: &str,
    newest_sha: &str,
) -> Result<()> {
    let pipelines = state
        .client
        .list_pipelines(project_id, Some(ref_name))
        .await?;

    for pipeline in pipelines {
        state
            .db
            .upsert_tracked_pipeline(&TrackedPipeline {
                pipeline_id: pipeline.id,
                project_id,
                ref_name: pipeline.ref_name.clone(),
                sha: pipeline.sha.clone(),
                status: pipeline.status.clone(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .await?;

        if pipeline.sha == newest_sha {
            continue;
        }

        if !matches!(pipeline.status.as_str(), "pending" | "running" | "created") {
            continue;
        }

        let decision = SupersedenceDecision {
            project_id,
            ref_name: ref_name.to_string(),
            newest_sha: newest_sha.to_string(),
            superseded_pipeline_id: pipeline.id,
            superseded_sha: pipeline.sha.clone(),
            action: SupersedenceAction::Cancel,
            reason: "newer commit superseded older in-flight pipeline on the same ref".to_string(),
        };

        state
            .db
            .append_event(
                "pipeline_superseded",
                Some(project_id),
                None,
                "engine",
                &serde_json::to_string(&decision)?,
            )
            .await?;

        state
            .client
            .cancel_pipeline(project_id, pipeline.id)
            .await?;
        state
            .db
            .append_event(
                "pipeline_cancel_requested",
                Some(project_id),
                None,
                "engine",
                &serde_json::json!({
                    "pipeline_id": pipeline.id,
                    "sha": pipeline.sha,
                    "ref_name": ref_name,
                })
                .to_string(),
            )
            .await?;
    }

    Ok(())
}

async fn maybe_secondary_attempt_failed_job(state: &EngineState, project_id: i64, job_id: i64) -> Result<()> {
    let Some(capsule) = state.db.latest_evidence_for_job(project_id, job_id).await? else {
        return Ok(());
    };

    let decision = capsule.recommended_recovery();
    let reason = format!(
        "{} / {}",
        capsule.failure_kind,
        format!("{:?}", capsule.classify()).to_ascii_lowercase()
    );

    state
        .db
        .insert_recovery_decision(
            project_id,
            job_id,
            &capsule.commit_sha,
            &capsule.ref_name,
            &format!("{:?}", decision).to_ascii_lowercase(),
            &reason,
        )
        .await?;

    if decision == RecoveryDecision::RetryOnce
        && state.db.count_recovery_decisions(project_id, job_id).await? == 1
    {
        aux_secondary::request_recovery_attempt(&state.client, project_id, job_id).await?;
        state
            .db
            .append_event(
                concat!("job_auto_", "ret", "ry_requested"),
                Some(project_id),
                Some(job_id),
                "engine",
                &serde_json::json!({
                    "job_id": job_id,
                    "commit_sha": capsule.commit_sha,
                    "ref_name": capsule.ref_name,
                    "reason": reason,
                })
                .to_string(),
            )
            .await?;
    }

    Ok(())
}

fn normalize_ref(value: &str) -> String {
    let stripped = match value.strip_prefix("refs/heads/") {
        Some(s) => Some(s),
        None => value.strip_prefix("refs/tags/"),
    };
    match stripped {
        Some(s) => s.to_string(),
        None => value.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Background tasks
// ---------------------------------------------------------------------------

async fn system_health_loop(state: SharedState) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static GC_RUNNING: AtomicBool = AtomicBool::new(false);

    // RAII guard to guarantee the lock is released even if an async task panics
    struct GcGuard;
    impl Drop for GcGuard {
        fn drop(&mut self) {
            GC_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    let mut auto_paused_pools: BTreeSet<String> = BTreeSet::new();
    let mut consecutive_zero_freed = 0u32;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // every 5 mins

    // Pre-flight disk check: if pressure is already critical, skip the settle delay
    if let Ok(fs) = crate::cache::df_usage("/").await {
        if fs.available_bytes >= crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES {
            // Safe to let engine settle
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        } else {
            warn!(
                root_free = %crate::cache::human_bytes(fs.available_bytes),
                required_free = %crate::cache::human_bytes(crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES),
                "startup pre-flight check detected disk pressure, bypassing settle delay"
            );
        }
    }

    loop {
        interval.tick().await;

        // Unconditional every-tick cleanup: kill orphaned Python forkserver workers
        // (processes reparented to init after their parent was SIGKILL'd).
        // Not gated on disk pressure — these consume RAM, not disk.
        let workers_killed = crate::reclaim::gc_orphaned_workers().await;
        if workers_killed > 0 {
            warn!("gc_orphaned_workers: killed {workers_killed} orphaned forkserver processes");
        }

        // Memory pressure check — escalates to active GC, not just logging
        let mem_gb = crate::reclaim::mem_available_gb();
        if mem_gb < 8.0 {
            error!("CRITICAL memory: {mem_gb:.1}GB available — forcing emergency GC");
            let _ = crate::reclaim::run_auto_gc(&state.docker, true, true).await;
        } else if mem_gb < 15.0 {
            warn!("memory pressure: {mem_gb:.1}GB available — triggering GC");
            let _ = crate::reclaim::run_auto_gc(&state.docker, false, false).await;
        }

        match crate::cache::df_usage("/").await {
            Ok(fs) => {
                let pressure = crate::cache::root_disk_pressure_level(fs.available_bytes);
                let root_free = fs.available_bytes;
                let root_used = fs.used_percent;

                if pressure == crate::cache::DiskPressureLevel::Nominal {
                    debug!(
                        root_free = %crate::cache::human_bytes(root_free),
                        root_used = root_used,
                        "disk pressure nominal"
                    );
                    consecutive_zero_freed = 0;

                    if !auto_paused_pools.is_empty() {
                        let paused: Vec<String> = auto_paused_pools.iter().cloned().collect();
                        for pool_name in paused {
                            if let Err(e) =
                                pool::resume_pool(&state.db, &state.client, &pool_name).await
                            {
                                error!(
                                    error = %e,
                                    pool = %pool_name,
                                    "failed to resume pool after disk pressure relief"
                                );
                            } else {
                                info!(
                                    pool = %pool_name,
                                    "resumed pool after disk pressure relief"
                                );
                                auto_paused_pools.remove(&pool_name);
                            }
                        }
                    }

                    let manager = crate::cache::CacheManager;
                    if let Err(e) = manager.gc_disk_cache().await {
                        error!(error = %e, "background GC failed");
                    }
                    // Proactively prune expired builder cache every cycle to prevent overlay2 accumulation.
                    let _ = tokio::process::Command::new("docker")
                        .args(["builder", "prune", "--force", "--filter", "until=2h"])
                        .output()
                        .await;
                    continue;
                }

                let is_critical = matches!(
                    pressure,
                    crate::cache::DiskPressureLevel::Critical
                        | crate::cache::DiskPressureLevel::Emergency
                );
                let is_emergency = pressure == crate::cache::DiskPressureLevel::Emergency;
                let is_warning = true;

                if GC_RUNNING.swap(true, Ordering::SeqCst) {
                    warn!("GC already in progress, skipping this cycle");
                    continue;
                }

                let _guard = GcGuard; // Will automatically reset GC_RUNNING to false on drop/panic

                if is_emergency {
                    warn!(
                        root_free = %crate::cache::human_bytes(root_free),
                        required_free = %crate::cache::human_bytes(
                            crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES
                        ),
                        "disk pressure emergency: pausing build/default pools and draining managers"
                    );

                    let pressure_pools = ["build", "default"];
                    for pool_name in pressure_pools {
                        if auto_paused_pools.contains(pool_name) {
                            continue;
                        }
                        if let Err(e) =
                            pool::drain_pool(&state.db, &state.docker, &state.client, pool_name)
                                .await
                        {
                            error!(
                                error = %e,
                                pool = pool_name,
                                "failed to drain pool during disk pressure emergency"
                            );
                        } else {
                            auto_paused_pools.insert(pool_name.to_string());
                            info!(pool = pool_name, "drained pool for emergency disk relief");
                        }
                    }

                    let _ = state
                        .db
                        .append_event(
                            "disk_pressure_emergency",
                            None,
                            None,
                            "system_health_loop",
                            &serde_json::json!({
                                "root_free_bytes": root_free,
                                "root_free_human": crate::cache::human_bytes(root_free),
                                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
                                "paused_pools": ["build", "default"],
                            })
                            .to_string(),
                        )
                        .await;
                } else {
                    warn!(
                        root_free = %crate::cache::human_bytes(root_free),
                        warning_floor = %crate::cache::human_bytes(
                            crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES
                        ),
                        "disk pressure warning: running cache GC"
                    );
                }

                let manager = crate::cache::CacheManager;
                if let Err(e) = manager
                    .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
                    .await
                {
                    error!(error = %e, "cache GC failed");
                }

                let mut current_free = root_free;
                let target_free_bytes = crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES;
                let mut pass = 0u32;
                let mut last_freed_bytes = u64::MAX;
                let usage_before = root_free;

                while current_free < target_free_bytes && pass < 20 {
                    pass += 1;
                    let escalated = pass > 1 || is_critical;
                    let free_before_pass = current_free;

                    warn!(
                        root_free = %crate::cache::human_bytes(current_free),
                        pass,
                        critical = escalated,
                        emergency = is_emergency,
                        "disk pressure: running GC pass"
                    );

                    let _ = state
                        .db
                        .append_event(
                            "disk_pressure_gc",
                            None,
                            None,
                            "system_health_loop",
                            &serde_json::json!({
                                "root_free_bytes": current_free,
                                "root_free_human": crate::cache::human_bytes(current_free),
                                "pass": pass,
                                "critical": escalated,
                                "emergency": is_emergency,
                                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
                            })
                            .to_string(),
                        )
                        .await;

                    if let Err(e) =
                        crate::reclaim::run_auto_gc(&state.docker, escalated, is_emergency).await
                    {
                        error!(error = %e, "auto_gc failed");
                        break;
                    }

                    let manager = crate::cache::CacheManager;
                    if let Err(e) = manager
                        .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
                        .await
                    {
                        error!(error = %e, "cache GC failed");
                    }

                    match crate::cache::df_usage("/").await {
                        Ok(fs) => current_free = fs.available_bytes,
                        Err(e) => {
                            warn!(error = %e, "failed to refresh disk usage after GC pass");
                            break;
                        }
                    }

                    let pass_freed = current_free.saturating_sub(free_before_pass);
                    if pass > 2
                        && pass_freed < 512 * 1024 * 1024
                        && last_freed_bytes < 512 * 1024 * 1024
                    {
                        warn!(
                            pass,
                            root_free = %crate::cache::human_bytes(current_free),
                            "GC stalled — two consecutive passes freed < 512MiB, stopping early"
                        );
                        break;
                    }
                    last_freed_bytes = pass_freed;

                    let pass_sleep = if is_emergency {
                        10
                    } else if is_critical {
                        20
                    } else {
                        30
                    };
                    tokio::time::sleep(std::time::Duration::from_secs(pass_sleep)).await;
                }

                let freed_bytes = current_free.saturating_sub(usage_before);
                let _ = state
                    .db
                    .append_event(
                        "disk_pressure_gc_complete",
                        None,
                        None,
                        "system_health_loop",
                        &serde_json::json!({
                            "root_free_before_bytes": usage_before,
                            "root_free_after_bytes": current_free,
                            "freed_bytes": freed_bytes,
                            "passes": pass,
                        })
                        .to_string(),
                    )
                    .await;

                if freed_bytes == 0 {
                    consecutive_zero_freed += 1;
                    if consecutive_zero_freed >= 3 {
                        error!(
                            consecutive_stalls = consecutive_zero_freed,
                            root_free = %crate::cache::human_bytes(current_free),
                            "disk GC stalled: 3 consecutive cycles freed near-zero space — manual intervention needed"
                        );
                        let _ = state
                            .db
                            .append_event(
                                "disk_gc_stalled",
                                None,
                                None,
                                "system_health_loop",
                                &serde_json::json!({
                                    "root_free_bytes": current_free,
                                    "consecutive_stalls": consecutive_zero_freed,
                                })
                                .to_string(),
                            )
                            .await;
                    }
                } else {
                    consecutive_zero_freed = 0;
                    info!(
                        freed_bytes,
                        root_free_after = %crate::cache::human_bytes(current_free),
                        "disk pressure relieved"
                    );
                }

                tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                continue;
            }
            Err(e) => {
                warn!(error = %e, "failed to check disk usage");
                continue;
            }
        };
    }
}

// ---------------------------------------------------------------------------
// Reconciliation loop
// ---------------------------------------------------------------------------

async fn reconciliation_loop(state: SharedState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));

    loop {
        interval.tick().await;
        debug!("running reconciliation");

        // Check each pool's desired vs actual manager count
        if let Err(e) = reconcile(&state).await {
            error!(error = %e, "reconciliation failed");
        }
    }
}

async fn reconcile(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    for p in &pools {
        if p.paused {
            continue;
        }

        let _stale_managers =
            pool::reconcile_manager_runtime_state(&state.db, &state.docker, Some(&p.name)).await?;
        let active = state.db.count_active_managers(&p.name).await?;

        let target = desired_manager_target(p, queued, running);

        // Scale to our exact target without ignoring running jobs.
        if active != target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                "reconciler: scaling pool"
            );
            pool::scale_pool_to(
                &state.db,
                &state.docker,
                &state.client,
                &p.name,
                target as usize,
            )
            .await?;
        }

        // Sync system_ids from disk for managers that don't have one yet
        let managers = state.db.list_managers(Some(&p.name)).await?;
        for m in &managers {
            if m.system_id.is_none() && (m.state == "starting" || m.state == "online") {
                let system_id_path = format!("{}/.runner_system_id", m.config_dir);
                if let Ok(sid) = std::fs::read_to_string(&system_id_path) {
                    let sid = sid.trim().to_string();
                    if !sid.is_empty() {
                        info!(manager_id = %m.id, system_id = %sid, "discovered system_id");
                        state.db.update_manager_system_id(&m.id, &sid).await?;
                        // Also mark as online since it's clearly registered
                        state.db.update_manager_state(&m.id, "online").await?;
                    }
                }
            }
        }
    }

    if let Err(err) = release::reconcile_release_for_ref(
        &state.db,
        &state.client,
        release::DEFAULT_RELEASE_PROJECT_ID,
        "main",
    )
    .await
    {
        warn!(error = %err, "release reconciliation failed");
    }

    Ok(())
}

fn desired_manager_target(pool: &crate::state::Pool, queued: i64, running: i64) -> i64 {
    let queue_target = pool.min_warm.saturating_add(queued).max(pool.min_warm);
    let active_work_target = queue_target.max(running);
    active_work_target.clamp(pool.min_warm, pool.max_managers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(min_warm: i64, max_managers: i64) -> crate::state::Pool {
        crate::state::Pool {
            name: "build".into(),
            gitlab_runner_id: 1,
            auth_token: "token".into(),
            tags: "build".into(),
            executor: "docker".into(),
            min_warm,
            max_managers,
            concurrent: 8,
            request_concurrency: 4,
            paused: false,
            trust_tier: "trusted".into(),
        }
    }

    #[test]
    fn desired_manager_target_accounts_for_running_jobs() {
        let pool = pool(1, 3);

        assert_eq!(desired_manager_target(&pool, 0, 0), 1);
        assert_eq!(desired_manager_target(&pool, 1, 0), 2);
        assert_eq!(desired_manager_target(&pool, 0, 3), 3);
        assert_eq!(desired_manager_target(&pool, 4, 4), 3);
    }
}

// ---------------------------------------------------------------------------
// Engine entry point
// ---------------------------------------------------------------------------

/// Start the engine (webhook server + reconciliation loop).
/// This runs indefinitely until the process is killed.
pub async fn run_engine(
    db: Db,
    docker: DockerCtl,
    client: GitlabClient,
    webhook_secret: String,
) -> Result<()> {
    let state = Arc::new(EngineState {
        db,
        docker,
        client,
        webhook_secret,
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/hooks", post(handle_webhook))
        .route("/cache/summary", get(cache_summary))
        .with_state(state.clone());

    // Start reconciliation loop
    let reconcile_state = state.clone();
    tokio::spawn(async move {
        reconciliation_loop(reconcile_state).await;
    });

    // Start Docker event listener loop (makes scaling instant)
    let event_state = state.clone();
    tokio::spawn(async move {
        docker_event_loop(event_state).await;
    });

    let addr = crate::settings::get().webhook.bind.clone();
    info!(addr = %addr, "starting jeryu engine");

    // Start background health sentinel loop
    let health_state = state.clone();
    tokio::spawn(async move {
        system_health_loop(health_state).await;
    });

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn cache_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let token = headers
        .get("X-Jeryu-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token != state.webhook_secret {
        warn!("cache_summary rejected: missing or invalid X-Jeryu-Token");
        return Err(StatusCode::UNAUTHORIZED);
    }
    let metrics = match state.db.get_cache_metrics().await {
        Ok(m) => m,
        Err(_) => Default::default(),
    };
    Ok(axum::Json(serde_json::json!({
        "bytes_served": metrics.bytes_served,
        "hits": metrics.hit_count,
        "objects": metrics.object_count,
        "status": "healthy"
    })))
}

// ---------------------------------------------------------------------------
// Docker Event Stream
// ---------------------------------------------------------------------------

async fn docker_event_loop(state: SharedState) {
    use futures_util::StreamExt;

    debug!("starting docker event listener");
    let mut stream = state.docker.events();

    while let Some(msg) = stream.next().await {
        if let Ok(event) = msg
            && let Some(typ) = event.typ
            && typ == bollard::models::EventMessageTypeEnum::CONTAINER
            && let Some(action) = event.action
        {
            // Check if it's a manager container dying or OOM
            if (action == "die" || action == "oom")
                && let Some(actor) = event.actor
                && let Some(attrs) = actor.attributes
                && attrs.get("jeryu.managed").map(|s| s.as_str()) == Some("true")
            {
                let name = match attrs.get("name").cloned() {
                    Some(n) => n,
                    None => String::new(),
                };
                warn!(%name, action, "jeryu manager container terminated unexpectedly");
                if let Some(manager_id) = attrs.get("jeryu.manager_id")
                    && let Err(error) = state.db.update_manager_state(manager_id, "stopped").await
                {
                    error!(%manager_id, %error, "failed to mark dead runner manager stopped");
                }
                // Run a full reconciliation immediately to replace it
                if let Err(e) = reconcile(&state).await {
                    error!(error = %e, "reconciliation failed after container death");
                }
            }
        }
    }
}
