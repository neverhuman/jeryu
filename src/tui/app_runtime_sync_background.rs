use super::*;

pub(crate) fn start_background_sync(app: &App) {
    let store = app.store.clone();
    let docker = app.docker.clone();
    let gitlab = app.gitlab.clone();
    let tx = app.sync_tx.clone();
    let log_tx = app.log_tx.clone();
    let mut log_rx = app.log_target_tx.subscribe();

    let store_flow = app.store.clone();
    let docker_flow = app.docker.clone();
    let gitlab_flow = app.gitlab.clone();

    let flow_tx = app.flow_tx.clone();
    let flow_log_rx = app.log_target_tx.subscribe();
    tokio::spawn(async move {
        crate::tui::flow::collector::run_collector(
            store_flow,
            docker_flow,
            gitlab_flow,
            flow_tx,
            flow_log_rx,
        )
        .await;
    });

    let gitlab_logs = app.gitlab.clone();
    tokio::spawn(async move {
        let mut current_target: Option<LogTarget> = None;
        let mut state = LiveLogState::default();
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(650));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            if let Ok(true) = log_rx.has_changed() {
                current_target = *log_rx.borrow_and_update();
                state = LiveLogState {
                    target: current_target,
                    ..Default::default()
                };
            }

            let Some(target) = current_target else {
                if state.target.is_some() {
                    state = LiveLogState::default();
                    if log_tx.send(state.clone()).await.is_err() {
                        break;
                    }
                }
                continue;
            };

            match gitlab_logs
                .job_trace(target.project_id, target.job_id)
                .await
            {
                Ok(trace_text) => {
                    state = LiveLogState {
                        target: Some(target),
                        text: retain_tail(&trace_text, LIVE_LOG_MAX_BYTES),
                        updated_at: Some(chrono::Utc::now().to_rfc3339()),
                        error: None,
                        outdated: false,
                    };
                }
                Err(error) => {
                    state.target = Some(target);
                    state
                        .updated_at
                        .get_or_insert_with(|| chrono::Utc::now().to_rfc3339());
                    state.error = Some(error.to_string());
                    state.outdated = true;
                }
            }

            if log_tx.send(state.clone()).await.is_err() {
                break;
            }
        }
    });

    // TUI v2 — Live Runner Feed background sync
    let store_feed = app.store.clone();
    let gitlab_feed = app.gitlab.clone();
    let feed_tx = app.feed_tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(2000));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            // Find running jobs
            let running_jobs = match store_feed.recent_job_events(50).await {
                Ok(jobs) => jobs
                    .into_iter()
                    .filter(|j| crate::tui::live::is_live_job_status(j.status.as_str()))
                    .take(5)
                    .collect::<Vec<_>>(),
                Err(_) => Vec::new(),
            };

            let mut feeds = Vec::new();
            for job in &running_jobs {
                let log_tail = match gitlab_feed.job_trace(job.project_id, job.job_id).await {
                    Ok(trace) => {
                        let lines: Vec<&str> = trace.lines().collect();
                        let start = lines.len().saturating_sub(FEED_MAX_LINES);
                        lines[start..].join("\n")
                    }
                    Err(_) => String::new(),
                };

                let elapsed = chrono::DateTime::parse_from_rfc3339(&job.received_at)
                    .map(|t| chrono::Utc::now().signed_duration_since(t).num_seconds() as f64)
                    .unwrap_or(0.0);

                feeds.push(RunnerFeed {
                    runner_name: match job.pool_name.clone() {
                        Some(value) => value,
                        None => "unknown".into(),
                    },
                    job_id: job.job_id,
                    job_name: match job.job_name.clone() {
                        Some(value) => value,
                        None => format!("job-{}", job.job_id),
                    },
                    pipeline_id: job.pipeline_id.unwrap_or(0),
                    status: job.status.clone(),
                    elapsed_secs: elapsed,
                    log_tail,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                });
            }

            if feed_tx.send(feeds).await.is_err() {
                break;
            }
        }
    });

    // TUI snapshot sync loop
    tokio::spawn(async move {
        loop {
            let mut snap = TuiStateSnapshot::default();
            App::hydrate_core_snapshot(&mut snap, &store, &docker, &gitlab).await;

            if let Ok(pipes) = store.list_tracked_pipelines(10).await {
                let mut pipe_metrics = Vec::new();
                let mut max_progress = 0;
                let mut latest_eta: Option<String> = None;

                for p in pipes {
                    // Count actual job events for this pipeline from persisted state
                    let total = store.count_pipeline_jobs_total(p.pipeline_id).await;
                    let completed = store.count_pipeline_jobs_completed(p.pipeline_id).await;
                    let running = store.count_pipeline_jobs_running(p.pipeline_id).await;

                    let pct = if total > 0 {
                        let effective = completed as f64 + (running as f64 * 0.5);
                        ((effective / total as f64) * 100.0) as u16
                    } else {
                        0
                    };

                    if (p.status == "running" || p.status == "pending" || p.status == "created")
                        && pct >= max_progress
                    {
                        max_progress = pct;
                        let remaining = total - completed;
                        // Estimate ~45 seconds per job (adjust heuristically based on audit suites)
                        let secs = remaining * 45;
                        latest_eta = Some(if secs > 3600 {
                            format!("~{}h {}m remaining", secs / 3600, (secs % 3600) / 60)
                        } else if secs > 60 {
                            format!("~{}m {}s remaining", secs / 60, secs % 60)
                        } else {
                            format!("~{}s remaining", secs)
                        });
                    }

                    pipe_metrics.push(PipelineMetrics {
                        pipeline: p.clone(),
                        total: total as usize,
                        completed: completed as usize,
                    });
                }
                snap.pipelines = pipe_metrics;
                snap.pipeline_progress = max_progress;
                snap.pipeline_eta = latest_eta;
            }

            snap.gitlab_ready = gitlab.is_ready().await;

            if let Ok(report) = release::build_release_status_report(
                &store,
                release::ReleaseStatusQuery {
                    project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                    ref_name: Some("main".into()),
                    sha: None,
                    limit: 1,
                },
            )
            .await
            {
                snap.release_status_generated_at = Some(report.generated_at);
                snap.release_status = report.latest;
            }

            // Observability metrics for Cache
            let proxy_addr = format!("127.0.0.1:{}", crate::config::CACHE_PROXY_PORT);
            let registry_addr = format!("127.0.0.1:{}", crate::config::CACHE_REGISTRY_PORT);

            snap.proxy_healthy = tokio::net::TcpStream::connect(&proxy_addr).await.is_ok();
            snap.registry_healthy = tokio::net::TcpStream::connect(&registry_addr).await.is_ok();

            let daemon_path = std::path::Path::new("/etc/docker/daemon.json");
            snap.mirror_enabled = if daemon_path.exists() {
                let content = match std::fs::read_to_string(daemon_path) {
                    Ok(s) => s,
                    Err(_) => String::new(),
                };
                content.contains(&registry_addr)
                    || content.contains(&crate::config::CACHE_REGISTRY_PORT.to_string())
            } else {
                false
            };

            snap.ca_mounted = std::path::Path::new("/etc/ssl/certs/ca-certificates.crt").exists();

            if let Ok(metrics) = store.get_cache_metrics().await {
                snap.hot_cache_usage_bytes = metrics.bytes_served;
                snap.cache_hits = metrics.hit_count;
                snap.cache_objects_count = metrics.object_count;
                snap.singleflight_requests = metrics.singleflight_coalesced;
                snap.hit_ratio = metrics.hit_ratio;
                snap.miss_count = metrics.miss_count;
                snap.total_requests = metrics.total_requests;
            }

            // Real taint and verdict counts from the state store
            snap.active_taint_count = store.count_cache_taints_total().await;
            snap.detonation_breaches = store.count_tripwire_taints().await;
            snap.cold_execution_downgrades = store.count_cache_misses().await;

            // Real disk usage for CAS and crate cache
            let cas_dir = crate::config::data_dir().join("cas");
            let crate_dir = crate::config::data_dir().join("cache").join("crates");
            snap.cas_disk_bytes = dir_size_bytes(&cas_dir).await;
            snap.crate_cache_disk_bytes = dir_size_bytes(&crate_dir).await;

            if let Ok(avg) = store.get_test_bottlenecks("average", 50).await {
                snap.test_bottlenecks_avg = avg;
            }
            if let Ok(lat) = store.get_test_bottlenecks("latest", 50).await {
                snap.test_bottlenecks_latest = lat;
            }

            if let Ok(evidence) = store.recent_evidence_all(30).await {
                snap.recent_evidence = evidence;
            }
            if let Ok(secrets) = store.all_recent_secret_audit_events(20).await {
                snap.secret_audit_events = secrets;
            }
            if let Ok(agent_pipes) = store.list_agent_pipelines().await {
                snap.agent_pipelines = agent_pipes;
            }
            if let Ok(events) = store.get_events(50).await {
                snap.recent_audit_events = events;
            }
            if let Ok(events) = store.recent_git_command_events(30).await {
                snap.recent_git_events = events;
            }
            snap.last_sync_at = Some(chrono::Utc::now());

            // Storage Metrics background queries
            if let Ok(df_output) = tokio::process::Command::new("df")
                .args(["-k", "/"])
                .output()
                .await
            {
                let s = String::from_utf8_lossy(&df_output.stdout);
                if let Some(line) = s.lines().nth(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        snap.storage_breakdown.total_disk_bytes =
                            parts[1].parse::<u64>().unwrap_or(0) * 1024;
                        snap.storage_breakdown.disk_available_bytes =
                            parts[3].parse::<u64>().unwrap_or(0) * 1024;
                    }
                }
            }

            // Fetch other storage queries roughly. (Since they are heavy, in a real app would cache them and do them every 60s instead of 1s, but this is a POC)
            snap.storage_breakdown.cas_bytes = snap.cas_disk_bytes as u64;
            snap.storage_breakdown.crate_cache_bytes = snap.crate_cache_disk_bytes as u64;
            snap.storage_breakdown.state_store_bytes =
                std::fs::metadata(crate::config::data_dir().join("jeryu.state"))
                    .map(|m| m.len())
                    .unwrap_or(0);

            snap.storage_breakdown.runner_data_bytes =
                dir_size_bytes(&crate::config::data_dir().join("runners")).await as u64;

            // Docker generic estimations (system df can be slow so we skip parsing detailed system df for this tight loop, substituting some approximations or just keeping them 0 for now)

            if tx.send(snap).await.is_err() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
    });
}
