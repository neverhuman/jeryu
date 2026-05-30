use std::path::Path;
use std::time::Instant;

use serde_json::json;

use super::*;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::pool::{self, PoolReservedNode, PoolTopologyPlan};
use crate::runner_backend::RunnerBackend;
use crate::runner_backend_remote;
use crate::runner_backend_remote::RemoteDockerBackend;
use crate::state::Db;

pub(crate) async fn collect_ci_checks(checks: &mut Vec<HealthCheck>) {
    checks.push(ci_runner_context_check());
    checks.push(ci_build_metadata_check());
    checks.push(ci_runner_tag_policy_check());
    checks.push(pipeline_doctor_schema_check().await);
    checks.push(tui_smoke_check().await);
}

pub(crate) async fn collect_local_checks(
    checks: &mut Vec<HealthCheck>,
    pool_topology: &mut Option<PoolTopologyPlan>,
    reserved_runner_nodes: &mut Vec<PoolReservedNode>,
) {
    checks.push(access_check());
    checks.push(installed_version_check().await);
    checks.push(root_disk_headroom_check().await);

    let db = match Db::open().await {
        Ok(db) => {
            checks.push(check(
                "database",
                HealthCheckStatus::Ok,
                "state database opened".to_string(),
                0,
                None,
            ));
            Some(db)
        }
        Err(err) => {
            checks.push(check(
                "database",
                HealthCheckStatus::Failed,
                format!("state database unavailable: {err}"),
                0,
                None,
            ));
            None
        }
    };

    let docker = match DockerCtl::connect() {
        Ok(docker) => {
            checks.push(check(
                "docker",
                HealthCheckStatus::Ok,
                "Docker client connected".to_string(),
                0,
                None,
            ));
            Some(docker)
        }
        Err(err) => {
            checks.push(check(
                "docker",
                HealthCheckStatus::Failed,
                format!("Docker unavailable: {err}"),
                0,
                None,
            ));
            None
        }
    };

    let gitlab_url = crate::gitlab_auth::default_gitlab_url();
    let token = crate::gitlab_auth::load_token_for_url(&gitlab_url)
        .ok()
        .flatten();
    let client = GitlabClient::new(&gitlab_url, token);
    let gitlab_ready = client.is_ready().await;
    checks.push(check(
        "gitlab",
        if gitlab_ready {
            HealthCheckStatus::Ok
        } else {
            HealthCheckStatus::Failed
        },
        format!(
            "{} {}",
            gitlab_url,
            if gitlab_ready {
                "reachable"
            } else {
                "not reachable"
            }
        ),
        0,
        None,
    ));

    if let Some(db) = db.as_ref() {
        checks.push(vault_check(db).await);
        checks.push(pipeline_doctor_schema_check().await);
    } else {
        checks.push(check(
            "vault",
            HealthCheckStatus::Skipped,
            "database unavailable; skipped Vault status".to_string(),
            0,
            None,
        ));
        checks.push(check(
            "pipeline_doctor_schema",
            HealthCheckStatus::Degraded,
            "database unavailable; schema probe skipped".to_string(),
            0,
            None,
        ));
    }

    match (db.as_ref(), docker.as_ref()) {
        (Some(db), Some(docker)) => {
            checks.push(host_doctor_check(db).await);
            let pool_check = pool_doctor_check(db, docker, &client).await;
            if let Some(data) = pool_check.data.as_ref()
                && let Some(topology_value) = data.get("topology")
                && let Ok(topology) =
                    serde_json::from_value::<PoolTopologyPlan>(topology_value.clone())
            {
                *pool_topology = Some(topology);
            }
            if let Some(data) = pool_check.data.as_ref()
                && let Some(reserved_value) = data.get("reserved_nodes")
                && let Ok(nodes) =
                    serde_json::from_value::<Vec<PoolReservedNode>>(reserved_value.clone())
            {
                *reserved_runner_nodes = nodes;
            }
            checks.push(pool_check);
            checks.push(runner_drift_check(db, docker).await);
            collect_node_checks(checks, db).await;
        }
        _ => {
            checks.push(check(
                "host_doctor",
                HealthCheckStatus::Skipped,
                "database or Docker unavailable; skipped host doctor".to_string(),
                0,
                None,
            ));
            checks.push(check(
                "pool_doctor",
                HealthCheckStatus::Skipped,
                "database or Docker unavailable; skipped pool doctor".to_string(),
                0,
                None,
            ));
            checks.push(check(
                "runners_drift",
                HealthCheckStatus::Skipped,
                "database or Docker unavailable; skipped runner drift check".to_string(),
                0,
                None,
            ));
        }
    }

    checks.push(tui_smoke_check().await);
}

fn access_check() -> HealthCheck {
    let started = Instant::now();
    let repo = match std::env::current_dir() {
        Ok(repo) => repo,
        Err(err) => {
            return check(
                "access",
                HealthCheckStatus::Failed,
                format!("could not resolve current directory: {err}"),
                started.elapsed().as_millis(),
                None,
            );
        }
    };
    let report = crate::access::load_contract()
        .and_then(|contract| crate::access::access_findings_for_repo(&contract, &repo));
    match report {
        Ok(report) => {
            let has_error = report
                .findings
                .iter()
                .any(|finding| finding.severity == "error");
            check(
                "access",
                if has_error {
                    HealthCheckStatus::Failed
                } else if report.findings.is_empty() {
                    HealthCheckStatus::Ok
                } else {
                    HealthCheckStatus::Degraded
                },
                if report.findings.is_empty() {
                    "access contract clean for current repo".to_string()
                } else {
                    format!("{} access finding(s)", report.findings.len())
                },
                started.elapsed().as_millis(),
                Some(json!({
                    "repo_path": report.repo_path,
                    "findings": report.findings,
                })),
            )
        }
        Err(err) => check(
            "access",
            HealthCheckStatus::Failed,
            format!("access doctor failed: {err}"),
            started.elapsed().as_millis(),
            None,
        ),
    }
}

async fn installed_version_check() -> HealthCheck {
    let started = Instant::now();
    let binary = crate::config::data_dir().join("bin/jeryu");
    if !binary.exists() {
        return check(
            "installed_version",
            HealthCheckStatus::Degraded,
            format!("installed binary not found at {}", binary.display()),
            started.elapsed().as_millis(),
            Some(json!({ "expected_binary": binary })),
        );
    }
    let output = tokio::process::Command::new(&binary)
        .arg("--version")
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let expected = env!("CARGO_PKG_VERSION");
            check(
                "installed_version",
                if version.contains(expected) {
                    HealthCheckStatus::Ok
                } else {
                    HealthCheckStatus::Degraded
                },
                format!("installed={version} expected={expected}"),
                started.elapsed().as_millis(),
                Some(json!({
                    "binary": binary,
                    "version_output": version,
                    "expected_version": expected,
                })),
            )
        }
        Ok(output) => check(
            "installed_version",
            HealthCheckStatus::Degraded,
            format!("installed binary exited with {}", output.status),
            started.elapsed().as_millis(),
            Some(json!({ "binary": binary })),
        ),
        Err(err) => check(
            "installed_version",
            HealthCheckStatus::Degraded,
            format!("could not run installed binary: {err}"),
            started.elapsed().as_millis(),
            Some(json!({ "binary": binary })),
        ),
    }
}

async fn vault_check(db: &Db) -> HealthCheck {
    let started = Instant::now();
    match crate::secrets::vault_status_observed(Some(db)).await {
        Ok(observed) => {
            let report = observed.report;
            let ok = observed.reachable
                && report.initialized
                && !report.sealed
                && report.healthy
                && report.token_present
                && Path::new(&report.bootstrap_file).exists()
                && Path::new(&report.env_file).exists();
            check(
                "vault",
                if ok {
                    HealthCheckStatus::Ok
                } else {
                    HealthCheckStatus::Failed
                },
                format!(
                    "reachable={} initialized={} sealed={} healthy={} token={}",
                    observed.reachable,
                    report.initialized,
                    report.sealed,
                    report.healthy,
                    report.token_present
                ),
                started.elapsed().as_millis(),
                Some(json!({
                    "addr": report.addr,
                    "reachable": observed.reachable,
                    "initialized": report.initialized,
                    "sealed": report.sealed,
                    "healthy": report.healthy,
                    "token_present": report.token_present,
                    "bootstrap_file": report.bootstrap_file,
                    "env_file": report.env_file,
                })),
            )
        }
        Err(err) => check(
            "vault",
            HealthCheckStatus::Failed,
            format!("Vault status failed: {err}"),
            started.elapsed().as_millis(),
            None,
        ),
    }
}

async fn host_doctor_check(db: &Db) -> HealthCheck {
    let started = Instant::now();
    match crate::cache::SmartCache::new(db.clone())
        .host_doctor_report()
        .await
    {
        Ok(report) => check(
            "host_doctor",
            if report.ok {
                HealthCheckStatus::Ok
            } else {
                HealthCheckStatus::Failed
            },
            format!(
                "{} host check(s), {} failing",
                report.checks.len(),
                report.checks.iter().filter(|check| !check.ok).count()
            ),
            started.elapsed().as_millis(),
            Some(json!({
                "ok": report.ok,
                "checks": report.checks,
                "root_available_bytes": report.cache.root_fs.available_bytes,
                "manager_cache_bytes": report.cache.manager_cache_bytes,
            })),
        ),
        Err(err) => check(
            "host_doctor",
            HealthCheckStatus::Failed,
            format!("host doctor failed: {err}"),
            started.elapsed().as_millis(),
            None,
        ),
    }
}

async fn pool_doctor_check(db: &Db, docker: &DockerCtl, client: &GitlabClient) -> HealthCheck {
    let started = Instant::now();
    match crate::pool::build_pool_doctor_report(db, docker, client).await {
        Ok(report) => check(
            "pool_doctor",
            if report.ok {
                HealthCheckStatus::Ok
            } else {
                HealthCheckStatus::Failed
            },
            {
                let (active_total, desired_total) =
                    report.topology.as_ref().map_or((0, 0), |topology| {
                        (topology.active_total, topology.desired_total)
                    });
                format!(
                    "standard topology active={} desired={} issues={}",
                    active_total,
                    desired_total,
                    report.issues.len()
                )
            },
            started.elapsed().as_millis(),
            Some(json!({
                "ok": report.ok,
                "issues": report.issues,
                "topology": report.topology,
                "reserved_nodes": report.reserved_nodes,
            })),
        ),
        Err(err) => check(
            "pool_doctor",
            HealthCheckStatus::Failed,
            format!("pool doctor failed: {err}"),
            started.elapsed().as_millis(),
            None,
        ),
    }
}

async fn runner_drift_check(db: &Db, docker: &DockerCtl) -> HealthCheck {
    let started = Instant::now();
    let pools = match db.list_pools().await {
        Ok(pools) => pools,
        Err(err) => {
            return check(
                "runners_drift",
                HealthCheckStatus::Failed,
                format!("runner drift check failed: could not list pools: {err}"),
                started.elapsed().as_millis(),
                None,
            );
        }
    };

    let mut db_active_total = 0_i64;
    let mut live_running_total = 0_i64;

    for pool_entry in &pools {
        match db.count_active_managers(&pool_entry.name).await {
            Ok(count) => db_active_total += count,
            Err(err) => {
                return check(
                    "runners_drift",
                    HealthCheckStatus::Failed,
                    format!(
                        "runner drift check failed: could not count active managers for pool '{}': {err}",
                        pool_entry.name
                    ),
                    started.elapsed().as_millis(),
                    None,
                );
            }
        }

        match pool::count_running_managers(db, docker, &pool_entry.name).await {
            Ok(count) => live_running_total += count,
            Err(err) => {
                return check(
                    "runners_drift",
                    HealthCheckStatus::Failed,
                    format!(
                        "runner drift check failed: could not count live managers for pool '{}': {err}",
                        pool_entry.name
                    ),
                    started.elapsed().as_millis(),
                    None,
                );
            }
        }
    }

    runner_drift_check_from_totals(
        db_active_total,
        live_running_total,
        pools.len(),
        started.elapsed().as_millis(),
    )
}

pub(crate) fn runner_drift_check_from_totals(
    db_active_total: i64,
    live_running_total: i64,
    pool_count: usize,
    duration_ms: u128,
) -> HealthCheck {
    let drift = live_running_total - db_active_total;
    let status = if drift == 0 || drift.abs() < 2 {
        HealthCheckStatus::Ok
    } else {
        HealthCheckStatus::Failed
    };
    let detail = if drift == 0 {
        format!(
            "runner fleet in sync (db_active={} live_running={})",
            db_active_total, live_running_total
        )
    } else if drift.abs() < 2 {
        format!(
            "runner fleet nearly in sync (db_active={} live_running={} delta={:+}; tolerated minor drift)",
            db_active_total, live_running_total, drift
        )
    } else {
        format!(
            "runner fleet drift detected (db_active={} live_running={} delta={:+})",
            db_active_total, live_running_total, drift
        )
    };
    check(
        "runners_drift",
        status,
        detail,
        duration_ms,
        Some(json!({
            "db_active_total": db_active_total,
            "live_running_total": live_running_total,
            "drift": drift,
            "pool_count": pool_count,
        })),
    )
}

async fn root_disk_headroom_check() -> HealthCheck {
    let started = Instant::now();
    match crate::cache::df_usage("/").await {
        Ok(usage) => root_disk_headroom_check_from_free_bytes(
            usage.available_bytes,
            started.elapsed().as_millis(),
        ),
        Err(err) => check(
            "root_disk_headroom",
            HealthCheckStatus::Failed,
            format!("root disk headroom check failed: {err}"),
            started.elapsed().as_millis(),
            None,
        ),
    }
}

pub(crate) fn root_disk_headroom_check_from_free_bytes(
    available_bytes: u64,
    duration_ms: u128,
) -> HealthCheck {
    let status = if available_bytes < HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES {
        HealthCheckStatus::Failed
    } else if available_bytes < HEALTH_ROOT_DISK_WARNING_MIN_FREE_BYTES {
        HealthCheckStatus::Degraded
    } else {
        HealthCheckStatus::Ok
    };
    let detail = if available_bytes < HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES {
        format!(
            "root disk critical: {} free on / (warning below {}, critical below {})",
            crate::cache::human_bytes(available_bytes),
            crate::cache::human_bytes(HEALTH_ROOT_DISK_WARNING_MIN_FREE_BYTES),
            crate::cache::human_bytes(HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES)
        )
    } else if available_bytes < HEALTH_ROOT_DISK_WARNING_MIN_FREE_BYTES {
        format!(
            "root disk warning: {} free on / (critical below {})",
            crate::cache::human_bytes(available_bytes),
            crate::cache::human_bytes(HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES)
        )
    } else {
        format!(
            "root disk headroom healthy: {} free on /",
            crate::cache::human_bytes(available_bytes)
        )
    };
    check(
        "root_disk_headroom",
        status,
        detail,
        duration_ms,
        Some(json!({
            "path": "/",
            "available_bytes": available_bytes,
            "warning_threshold_bytes": HEALTH_ROOT_DISK_WARNING_MIN_FREE_BYTES,
            "critical_threshold_bytes": HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES,
            "pressure": if available_bytes < HEALTH_ROOT_DISK_CRITICAL_MIN_FREE_BYTES {
                "critical"
            } else if available_bytes < HEALTH_ROOT_DISK_WARNING_MIN_FREE_BYTES {
                "warning"
            } else {
                "nominal"
            },
        })),
    )
}

pub(crate) fn runner_utilization_summary_from_checks(
    checks: &[HealthCheck],
) -> (Option<f64>, Option<usize>, Option<usize>) {
    let Some(data) = checks
        .iter()
        .find(|check| check.id == "runners_drift")
        .and_then(|check| check.data.as_ref())
    else {
        return (None, None, None);
    };

    let Some(db_active_total) = data.get("db_active_total").and_then(|value| value.as_i64()) else {
        return (None, None, None);
    };
    let Some(live_running_total) = data
        .get("live_running_total")
        .and_then(|value| value.as_i64())
    else {
        return (None, None, None);
    };

    let (ratio, idle_count, stuck_count) =
        runner_utilization_from_totals(db_active_total, live_running_total);
    (Some(ratio), Some(idle_count), Some(stuck_count))
}

pub(crate) fn runner_utilization_from_totals(
    db_active_total: i64,
    live_running_total: i64,
) -> (f64, usize, usize) {
    let ratio = if db_active_total > 0 {
        live_running_total as f64 / db_active_total as f64
    } else {
        0.0
    };
    let idle_count = if live_running_total > db_active_total {
        (live_running_total - db_active_total) as usize
    } else {
        0
    };
    let stuck_count = if db_active_total > live_running_total {
        (db_active_total - live_running_total) as usize
    } else {
        0
    };
    (ratio, idle_count, stuck_count)
}

async fn collect_node_checks(checks: &mut Vec<HealthCheck>, db: &Db) {
    for alias in ["xbabe0", "xbabe1", "xbabe2", "xbabe3"] {
        let started = Instant::now();
        let reserved = crate::config::STANDARD_POOL_RESERVED_NODE_ALIASES.contains(&alias);
        let expected = if reserved { 0 } else { 10 };
        let cfg = match crate::node_support::load_node_config(alias) {
            Ok(cfg) => cfg,
            Err(err) => {
                checks.push(check(
                    &format!("node_{alias}"),
                    if reserved {
                        HealthCheckStatus::Degraded
                    } else {
                        HealthCheckStatus::Failed
                    },
                    format!("node config missing/unreadable: {err}"),
                    started.elapsed().as_millis(),
                    Some(json!({ "alias": alias, "expected_managers": expected })),
                ));
                continue;
            }
        };
        let db_active = match db.list_managers_for_node(alias).await {
            Ok(managers) => managers
                .iter()
                .filter(|manager| {
                    matches!(
                        manager.state.as_str(),
                        "starting" | "online" | "node_starting" | "node_unreachable" | "draining"
                    )
                })
                .count(),
            Err(_) => 0,
        };
        let live_running = match live_running_managers_on_node(&cfg).await {
            Ok(count) => count,
            Err(err) => {
                checks.push(check(
                    &format!("node_{alias}"),
                    HealthCheckStatus::Failed,
                    format!("could not inspect live node inventory: {err}"),
                    started.elapsed().as_millis(),
                    Some(json!({
                        "alias": alias,
                        "reserved": reserved,
                        "target": cfg.target,
                        "enabled": cfg.enabled,
                        "db_active_managers": db_active,
                        "expected_managers": expected,
                    })),
                ));
                continue;
            }
        };
        let probe = runner_backend_remote::probe_node(&cfg).await;
        let over_capacity =
            db_active > cfg.max_managers as usize || live_running > cfg.max_managers as usize;
        let ok = probe.reachable
            && probe.docker_ready
            && db_active == expected
            && live_running == expected
            && !over_capacity;
        checks.push(check(
            &format!("node_{alias}"),
            if ok {
                HealthCheckStatus::Ok
            } else {
                HealthCheckStatus::Failed
            },
            format!(
                "reachable={} docker={} db_active={} live_running={} expected={}",
                probe.reachable, probe.docker_ready, db_active, live_running, expected
            ),
            started.elapsed().as_millis(),
            Some(json!({
                "alias": alias,
                "reserved": reserved,
                "target": cfg.target,
                "enabled": cfg.enabled,
                "db_active_managers": db_active,
                "live_running_managers": live_running,
                "expected_managers": expected,
                "over_capacity": over_capacity,
                "reachable": probe.reachable,
                "docker_ready": probe.docker_ready,
                "disk_free_gb": probe.disk_free_gb,
                "os": probe.os,
                "arch": probe.arch,
            })),
        ));
    }
}

async fn live_running_managers_on_node(
    cfg: &crate::node_types::NodeConfig,
) -> anyhow::Result<usize> {
    let backend = RemoteDockerBackend::new(cfg.clone());
    Ok(backend
        .list_managed_containers()
        .await?
        .into_iter()
        .filter(|container| container.running)
        .count())
}

async fn pipeline_doctor_schema_check() -> HealthCheck {
    let started = Instant::now();
    let context = crate::release::pipeline_doctor_schema_context().await;
    check(
        "pipeline_doctor_schema",
        if context.available {
            HealthCheckStatus::Ok
        } else {
            HealthCheckStatus::Degraded
        },
        if context.available {
            format!(
                "{} available ({} job definitions)",
                context.source, context.job_count
            )
        } else {
            format!(
                "{} unavailable: {}",
                context.source,
                context
                    .degraded_reason
                    .as_deref()
                    .unwrap_or("unknown reason")
            )
        },
        started.elapsed().as_millis(),
        Some(json!(context)),
    )
}

async fn tui_smoke_check() -> HealthCheck {
    let started = Instant::now();
    let client = GitlabClient::new("http://127.0.0.1:9", None);
    match crate::tui::smoke_render_once(None, DockerCtl::disconnected(), client, "jobs").await {
        Ok(live_jobs) => check(
            "tui_smoke",
            HealthCheckStatus::Ok,
            format!("TUI smoke render ok (live jobs: {live_jobs})"),
            started.elapsed().as_millis(),
            None,
        ),
        Err(err) => check(
            "tui_smoke",
            HealthCheckStatus::Failed,
            format!("TUI smoke render failed: {err}"),
            started.elapsed().as_millis(),
            None,
        ),
    }
}

fn ci_runner_context_check() -> HealthCheck {
    let context = CiRunnerContext::from_env();
    check(
        "ci_runner_context",
        context.status,
        context.detail,
        0,
        Some(context.data),
    )
}

fn ci_build_metadata_check() -> HealthCheck {
    let metadata = CiBuildMetadata::from_env();
    check(
        "ci_build_metadata",
        HealthCheckStatus::Ok,
        format!(
            "jeryu {} ref={} sha={}",
            env!("CARGO_PKG_VERSION"),
            metadata.ref_name.as_deref().unwrap_or("local"),
            metadata.sha.as_deref().unwrap_or("unknown")
        ),
        0,
        Some(metadata.data),
    )
}

fn ci_runner_tag_policy_check() -> HealthCheck {
    let tags = env_var("CI_RUNNER_TAGS");
    let trimmed = match tags.as_deref() {
        Some(value) => value.trim(),
        None => "",
    };
    check(
        "ci_runner_tag_policy",
        if trimmed.is_empty() {
            HealthCheckStatus::Ok
        } else {
            HealthCheckStatus::Failed
        },
        if trimmed.is_empty() {
            "runner has no CI tags; standard jobs stay untagged".to_string()
        } else {
            format!("runner has forbidden CI tags: {trimmed}")
        },
        0,
        Some(json!({ "ci_runner_tags": tags })),
    )
}

#[derive(Debug, Clone)]
struct CiRunnerContext {
    status: HealthCheckStatus,
    detail: String,
    data: serde_json::Value,
}

impl CiRunnerContext {
    fn from_env() -> Self {
        let gitlab_ci = env_var("GITLAB_CI");
        let job_id = env_var("CI_JOB_ID");
        let runner_id = env_var("CI_RUNNER_ID");
        let runner_description = env_var("CI_RUNNER_DESCRIPTION");
        let is_ci = gitlab_ci.as_deref() == Some("true") || std::env::var("CI").is_ok();
        let status = if is_ci && job_id.is_some() {
            HealthCheckStatus::Ok
        } else {
            HealthCheckStatus::Degraded
        };
        let detail = if is_ci {
            format!(
                "GitLab CI job={} runner={}",
                job_id.as_deref().unwrap_or("<unset>"),
                runner_id.as_deref().unwrap_or("<unset>")
            )
        } else {
            "not running under GitLab CI".to_string()
        };
        Self {
            status,
            detail,
            data: json!({
                "mode": if is_ci { "ci" } else { "local" },
                "gitlab_ci": gitlab_ci,
                "ci_job_id": job_id,
                "ci_runner_id": runner_id,
                "ci_runner_description": runner_description,
            }),
        }
    }
}

#[derive(Debug, Clone)]
struct CiBuildMetadata {
    ref_name: Option<String>,
    sha: Option<String>,
    data: serde_json::Value,
}

impl CiBuildMetadata {
    fn from_env() -> Self {
        let pipeline_id = env_var("CI_PIPELINE_ID");
        let job_id = env_var("CI_JOB_ID");
        let ref_name = env_var("CI_COMMIT_REF_NAME");
        let sha = env_var("CI_COMMIT_SHA");
        let source = env_var("CI_PIPELINE_SOURCE");
        Self {
            ref_name: ref_name.clone(),
            sha: sha.clone(),
            data: json!({
                "crate_version": env!("CARGO_PKG_VERSION"),
                "pipeline_id": pipeline_id,
                "job_id": job_id,
                "ref_name": ref_name,
                "sha": sha,
                "source": source,
            }),
        }
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

pub(crate) fn component_from_check(
    name: &str,
    report: &HealthReport,
    check_id: &str,
) -> ComponentHealth {
    let Some(check) = report.checks.iter().find(|check| check.id == check_id) else {
        return ComponentHealth::unknown(name);
    };
    let status = match check.status {
        HealthCheckStatus::Ok => HealthLevel::Healthy,
        HealthCheckStatus::Degraded | HealthCheckStatus::Skipped => HealthLevel::Degraded,
        HealthCheckStatus::Failed => HealthLevel::Critical,
    };
    ComponentHealth {
        name: name.to_string(),
        status,
        latency_ms: Some(check.duration_ms.try_into().unwrap_or(u64::MAX)),
        detail: Some(check.detail.clone()),
    }
}

pub(crate) fn check(
    id: &str,
    status: HealthCheckStatus,
    detail: String,
    duration_ms: u128,
    data: Option<serde_json::Value>,
) -> HealthCheck {
    HealthCheck {
        id: id.to_string(),
        status,
        detail,
        duration_ms,
        data,
    }
}
