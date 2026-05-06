//! Owner: Capability API (Structured AgentIntent Payloads)
//! Proof: `cargo check -p jeryu` (type-checked surface)
//! Invariants: All external agent payloads deserialize through AgentIntent or AgentActionRequest
//!             before execution; AgentIntent uses tagged serde enum; never execute raw payload strings.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Mutex, OnceLock};

const CAPABILITY_PROTOCOL_VERSION: &str = "v3.01";
const MAX_CAPABILITY_FRAME_BYTES: usize = 1024 * 1024;

static SEEN_NONCES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "intent", content = "payload")]
pub enum AgentIntent {
    ProposePatch {
        project_id: i64,
        branch_name: String,
        base_ref: String,
        commit_message: String,
        modifications: Vec<FileModification>,
        mr_title: Option<String>,
    },
    RacePatches {
        project_id: i64,
        base_branch: String,
        commit_message: String,
        hypotheses: Vec<HypothesisPatch>,
    },
    RunTests {
        project_id: i64,
        target_ref: String,
        test_scope: String,
    },
    FetchCapsule {
        job_id: i64,
    },
    RequestMerge {
        project_id: i64,
        mr_iid: i64,
        source_branch: String,
        target_branch: String,
    },
    ExplainBlockers {
        entity_type: String, // "job" | "release" | "merge"
        entity_id: i64,
    },
    GetSystemSnapshot,
    GetPipelineJobs {
        project_id: i64,
        pipeline_id: i64,
    },
    GetCiBottlenecks {
        project_id: i64,
        ref_name: Option<String>,
        limit: Option<i64>,
    },
    ListAllowedActions,
    PlanValidation {
        project_id: i64,
        test_ids: Vec<String>,
        ref_name: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentActionRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub actor: String,
    pub nonce: String,
    pub expires_at: Option<String>,
    pub project_id: Option<i64>,
    pub base_ref: Option<String>,
    pub base_sha: Option<String>,
    pub idempotency_key: Option<String>,
    pub budget: Option<ActionBudget>,
    pub grant: Option<CapabilityGrantProof>,
    pub intent: AgentIntent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActionBudget {
    pub max_seconds: Option<u64>,
    pub max_branch_writes: Option<u32>,
    pub max_pipeline_triggers: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CapabilityGrantProof {
    pub grant_id: String,
    pub actor: String,
    pub action_id: String,
    pub scope: CapabilityGrantScope,
    pub issued_at: String,
    pub expires_at: String,
    pub signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CapabilityGrantScope {
    pub project_id: Option<i64>,
    pub refs: Vec<String>,
    pub paths: Vec<String>,
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CapabilityContext {
    request_id: String,
    actor: String,
    protocol_version: String,
    compat: bool,
}

impl CapabilityContext {
    pub(crate) fn compat() -> Self {
        Self {
            request_id: format!("compat-{}", uuid::Uuid::new_v4()),
            actor: "compat-capability-client".to_string(),
            protocol_version: "compat-agent-intent".to_string(),
            compat: true,
        }
    }

    pub(crate) fn mcp(request_id: String, actor: String, protocol_version: String) -> Self {
        Self {
            request_id,
            actor,
            protocol_version,
            compat: false,
        }
    }
}

#[derive(Debug)]
enum ParsedCapabilityRequest {
    Enveloped(Box<AgentActionRequest>),
    Compat(AgentIntent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileModification {
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HypothesisPatch {
    pub branch_suffix: String,
    pub modifications: Vec<FileModification>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CapabilityResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct CiJob<'a> {
    stage: &'a str,
    tags: Vec<&'a str>,
    script: Vec<&'a str>,
}

fn dynamic_ci_yaml(scope: &str) -> anyhow::Result<String> {
    let (job_suffix, script) = match scope {
        "unit" => ("unit", vec!["cargo test --lib --benches"]),
        "integration" => ("integration", vec!["cargo test --test '*'"]),
        "lint" => (
            "lint",
            vec![
                "cargo clippy --all-targets --all-features -- -D warnings",
                "cargo fmt -- --check",
            ],
        ),
        "full" => ("full", vec!["cargo test"]),
        other => {
            anyhow::bail!(
                "unsupported test scope '{other}'; allowed scopes: unit, integration, lint, full"
            )
        }
    };

    let mut doc = BTreeMap::new();
    doc.insert("image".to_string(), serde_yaml::to_value("rust:latest")?);
    doc.insert("stages".to_string(), serde_yaml::to_value(vec!["test"])?);
    doc.insert(
        format!("dynamic-{job_suffix}-job"),
        serde_yaml::to_value(CiJob {
            stage: "test",
            tags: vec!["jeryu"],
            script,
        })?,
    );
    Ok(serde_yaml::to_string(&doc)?)
}

/// Start the Capability Unix Domain Socket Server.
pub async fn start_capability_server(
    socket_path: &str,
    client: crate::gitlab_client::GitlabClient,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("Capability server listening on {}", socket_path);

    loop {
        match listener.accept().await {
            Ok((mut stream, _addr)) => {
                let client = client.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    if let Ok(n) = stream.read(&mut buf).await {
                        if n == 0 {
                            return;
                        }
                        let resp = match parse_capability_request(&buf[..n])
                            .and_then(validate_capability_request)
                        {
                            Ok((intent, ctx)) => {
                                tracing::info!(
                                    request_id = %ctx.request_id,
                                    actor = %ctx.actor,
                                    protocol_version = %ctx.protocol_version,
                                    compat = ctx.compat,
                                    "capability request accepted"
                                );
                                execute_intent(intent, &ctx, &client).await
                            }
                            Err(e) => CapabilityResponse {
                                success: false,
                                message: format!("invalid capability request: {}", e),
                                data: None,
                            },
                        };
                        let _ = stream.write_all(&serde_json::to_vec(&resp).unwrap()).await;
                    }
                });
            }
            Err(e) => {
                tracing::error!("capability accept error: {}", e);
            }
        }
    }
}

pub(crate) async fn execute_intent(
    intent: AgentIntent,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match intent {
        // ── FetchCapsule ──────────────────────────────────────────────────────
        AgentIntent::FetchCapsule { job_id } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };
            match db.latest_evidence_by_job_id(job_id).await {
                Ok(Some(cap)) => CapabilityResponse {
                    success: true,
                    message: "Capsule retrieved".into(),
                    data: serde_json::to_value(cap).ok(),
                },
                Ok(None) => err(&format!("no capsule found for job_id={}", job_id)),
                Err(e) => err(&format!("db error: {}", e)),
            }
        }

        // ── RunTests ──────────────────────────────────────────────────────────
        AgentIntent::RunTests {
            project_id,
            target_ref,
            test_scope,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };
            let ts = chrono::Utc::now().timestamp_millis();
            let branch = format!("{}-ci-{}", target_ref, ts);

            if let Err(e) = client.create_branch(project_id, &branch, &target_ref).await {
                return err(&format!("create_branch: {}", e));
            }
            let yaml = match dynamic_ci_yaml(&test_scope) {
                Ok(yaml) => yaml,
                Err(e) => return err(&format!("ci template: {e}")),
            };
            let commit_sha = match client
                .commit_actions_with_sha(
                    project_id,
                    &branch,
                    &format!("DAG injection for {}", test_scope),
                    &[("update", ".gitlab-ci.yml", yaml.as_str())],
                )
                .await
            {
                Ok(sha) => sha,
                Err(e) => return err(&format!("update_file: {}", e)),
            };
            let pipeline_id = match client.trigger_pipeline(project_id, &branch, vec![]).await {
                Ok(pipeline_id) => pipeline_id,
                Err(e) => return err(&format!("trigger_pipeline: {}", e)),
            };
            let grant = record_branch_capability_grant(
                &db,
                "RunTests",
                "run_tests",
                ctx,
                project_id,
                &branch,
                Some(&target_ref),
                Some(&commit_sha),
                serde_json::json!({
                    "project_id": project_id,
                    "target_ref": target_ref,
                    "test_scope": test_scope,
                    "branch": branch,
                    "commit_sha": commit_sha.clone(),
                    "pipeline_id": pipeline_id,
                }),
            )
            .await
            .ok();
            CapabilityResponse {
                success: true,
                message: format!("ephemeral test branch created: {}", branch),
                data: Some(
                    serde_json::json!({"branch": branch, "scope": test_scope, "pipeline_id": pipeline_id, "grant_id": grant}),
                ),
            }
        }

        // ── ProposePatch ──────────────────────────────────────────────────────
        AgentIntent::ProposePatch {
            project_id,
            branch_name,
            base_ref,
            commit_message,
            modifications,
            mr_title,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };
            if let Err(e) = client
                .create_branch(project_id, &branch_name, &base_ref)
                .await
            {
                return err(&format!("create_branch: {}", e));
            }

            // Apply all file modifications via a single multi-action commit.
            // commit_actions takes &[(&str, &str, &str)] = (action, path, content).
            let tuples: Vec<(&str, &str, &str)> = modifications
                .iter()
                .map(|m| ("update", m.file_path.as_str(), m.content.as_str()))
                .collect();

            let commit_sha = match client
                .commit_actions_with_sha(project_id, &branch_name, &commit_message, &tuples)
                .await
            {
                Ok(sha) => sha,
                Err(e) => return err(&format!("commit_actions: {}", e)),
            };

            let title = mr_title.unwrap_or_else(|| commit_message.clone());
            match client
                .create_merge_request(project_id, &branch_name, &base_ref, &title, "")
                .await
            {
                Ok(mr) => {
                    let grant = record_branch_capability_grant(
                        &db,
                        "ProposePatch",
                        "propose_patch",
                        ctx,
                        project_id,
                        &branch_name,
                        Some(&base_ref),
                        Some(&commit_sha),
                        serde_json::json!({
                            "project_id": project_id,
                            "branch": branch_name,
                            "base_ref": base_ref,
                            "commit_sha": commit_sha.clone(),
                            "mr_iid": mr.iid,
                            "mr_url": mr.web_url,
                            "files_changed": modifications.len(),
                        }),
                    )
                    .await
                    .ok();
                    CapabilityResponse {
                        success: true,
                        message: format!("MR !{} created on branch {}", mr.iid, branch_name),
                        data: Some(serde_json::json!({
                            "branch": branch_name,
                            "mr_iid": mr.iid,
                            "mr_url": mr.web_url,
                            "grant_id": grant,
                        })),
                    }
                }
                Err(e) => err(&format!("create_merge_request: {}", e)),
            }
        }

        // ── RacePatches ───────────────────────────────────────────────────────
        AgentIntent::RacePatches {
            project_id,
            base_branch,
            commit_message,
            hypotheses,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };
            if hypotheses.is_empty() {
                return err("hypotheses list is empty");
            }

            // Create all branches and kick off pipelines in parallel
            let mut branch_pipeline_pairs: Vec<(String, Option<i64>)> = Vec::new();
            let mut grant_ids: Vec<String> = Vec::new();
            for h in &hypotheses {
                let branch = format!("{}-race-{}", base_branch, h.branch_suffix);
                if client
                    .create_branch(project_id, &branch, &base_branch)
                    .await
                    .is_err()
                {
                    continue;
                }
                let tuples: Vec<(&str, &str, &str)> = h
                    .modifications
                    .iter()
                    .map(|m| ("update", m.file_path.as_str(), m.content.as_str()))
                    .collect();
                let Ok(commit_sha) = client
                    .commit_actions_with_sha(project_id, &branch, &commit_message, &tuples)
                    .await
                else {
                    continue;
                };
                let pid = client
                    .trigger_pipeline(project_id, &branch, vec![])
                    .await
                    .ok();
                if let Ok(grant_id) = record_branch_capability_grant(
                    &db,
                    "RacePatches",
                    "race_patches",
                    ctx,
                    project_id,
                    &branch,
                    Some(&base_branch),
                    Some(&commit_sha),
                    serde_json::json!({
                        "project_id": project_id,
                        "base_branch": base_branch,
                        "branch": branch,
                        "branch_suffix": h.branch_suffix,
                        "pipeline_id": pid,
                        "commit_sha": commit_sha.clone(),
                        "files_changed": h.modifications.len(),
                    }),
                )
                .await
                {
                    grant_ids.push(grant_id);
                }
                branch_pipeline_pairs.push((branch, pid));
            }

            if branch_pipeline_pairs.is_empty() {
                return err("failed to create any hypothesis branches");
            }

            CapabilityResponse {
                success: true,
                message: format!(
                    "{} hypothesis branches launched; monitor pipelines for winner",
                    branch_pipeline_pairs.len()
                ),
                data: Some(serde_json::json!({
                    "branches": branch_pipeline_pairs.iter().map(|(b, pid)| serde_json::json!({"branch": b, "pipeline_id": pid})).collect::<Vec<_>>(),
                    "grant_ids": grant_ids,
                    "status": "racing",
                    "note": "Poll pipeline status to determine winner; losing branches will need manual cleanup or implement PollRaceResult."
                })),
            }
        }

        // ── RequestMerge ──────────────────────────────────────────────────────
        AgentIntent::RequestMerge {
            project_id,
            mr_iid,
            source_branch,
            target_branch,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };

            let mut blockers: Vec<String> = Vec::new();

            // Check 1: selector miss count since branch creation
            let since_24h = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::hours(24))
                .unwrap_or(chrono::Utc::now())
                .to_rfc3339();
            let miss_count = db
                .count_selector_misses_since(&since_24h)
                .await
                .unwrap_or(0);
            if miss_count > 0 {
                blockers.push(format!(
                    "test selector has {} unresolved miss(es) in the last 24h",
                    miss_count
                ));
            }

            // Check 2: active cache taints
            let taint_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cache_taints")
                .fetch_one(&db.pool())
                .await
                .unwrap_or(0);
            if taint_count > 0 {
                blockers.push(format!("{} active cache taint(s)", taint_count));
            }

            let proof = crate::decision::evaluate_merge_gate(
                crate::decision::MergeGateInput {
                    project_id,
                    mr_iid,
                    source_branch,
                    target_branch,
                    head_sha: None,
                    successful_jobs: 0,
                    pending_jobs: 0,
                    failed_jobs: 0,
                    selector_misses: miss_count as usize,
                    cache_taints: taint_count as usize,
                    vti_receipt: None,
                    trust_tier: crate::decision::TrustTier::Trusted,
                },
                &crate::decision::RequiredEvidencePolicy::default(),
            );

            if proof.decision == crate::decision::RiskGateDecision::Allow {
                CapabilityResponse {
                    success: true,
                    message: format!("merge !{} allowed — all gates pass", mr_iid),
                    data: serde_json::to_value(proof).ok(),
                }
            } else {
                CapabilityResponse {
                    success: false,
                    message: format!("merge !{} denied: {}", mr_iid, blockers.join("; ")),
                    data: serde_json::to_value(proof).ok(),
                }
            }
        }

        // ── ExplainBlockers ───────────────────────────────────────────────────
        AgentIntent::ExplainBlockers {
            entity_type,
            entity_id,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };

            let explanation = match entity_type.as_str() {
                "job" => match db.latest_evidence_by_job_id(entity_id).await {
                    Ok(Some(cap)) => format!(
                        "Job #{} failed in stage '{}' with exit code {}.\nFailure kind: {}.\nSummary: {}\nReproduce: {}",
                        entity_id,
                        cap.stage,
                        cap.exit_code,
                        cap.failure_kind,
                        cap.summary,
                        if cap.repro_script.len() > 200 {
                            &cap.repro_script[..200]
                        } else {
                            &cap.repro_script
                        }
                    ),
                    Ok(None) => format!(
                        "Job #{} has no failure capsule. It may not have failed, or evidence was not captured.",
                        entity_id
                    ),
                    Err(e) => format!("Failed to query job {}: {}", entity_id, e),
                },
                "release" => match db.recent_evidence_all(5).await {
                    Ok(records) if !records.is_empty() => {
                        let lines: Vec<String> = records
                            .iter()
                            .map(|r| {
                                format!(
                                    "  job#{} {} exit:{} ({})",
                                    r.job_id, r.failure_kind, r.exit_code, r.stage
                                )
                            })
                            .collect();
                        format!(
                            "Release #{} blockers — recent failures:\n{}",
                            entity_id,
                            lines.join("\n")
                        )
                    }
                    _ => format!(
                        "Release #{} — no recent failures found in evidence capsules.",
                        entity_id
                    ),
                },
                _ => format!(
                    "Unknown entity_type '{}'. Use: job | release | merge",
                    entity_type
                ),
            };

            CapabilityResponse {
                success: true,
                message: "Blocker explanation generated".into(),
                data: Some(
                    serde_json::json!({"explanation": explanation, "entity_type": entity_type, "entity_id": entity_id}),
                ),
            }
        }

        // ── GetSystemSnapshot ─────────────────────────────────────────────────
        AgentIntent::GetSystemSnapshot => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };

            let pools = db.list_pools().await.unwrap_or_default();
            let metrics = db.get_cache_metrics().await.unwrap_or_default();
            let since_1h = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::hours(1))
                .unwrap_or(chrono::Utc::now())
                .to_rfc3339();
            let miss_count = db.count_selector_misses_since(&since_1h).await.unwrap_or(0);
            let recent_capsules = db.recent_evidence_all(5).await.unwrap_or_default();

            let pool_summary: Vec<serde_json::Value> = pools
                .iter()
                .map(|p| serde_json::json!({"name": p.name, "paused": p.paused, "min_warm": p.min_warm}))
                .collect();
            let capsule_summary: Vec<serde_json::Value> = recent_capsules
                .iter()
                .map(|c| serde_json::json!({"job_id": c.job_id, "failure_kind": c.failure_kind, "exit_code": c.exit_code}))
                .collect();

            CapabilityResponse {
                success: true,
                message: "System snapshot".into(),
                data: Some(serde_json::json!({
                    "pools": pool_summary,
                    "cache": {
                        "hits": metrics.hit_count,
                        "misses": metrics.miss_count,
                        "objects": metrics.object_count,
                        "hit_ratio": metrics.hit_ratio,
                    },
                    "selector_misses_last_hour": miss_count,
                    "recent_failures": capsule_summary,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                })),
            }
        }

        // ── GetPipelineJobs ──────────────────────────────────────────────────
        AgentIntent::GetPipelineJobs {
            project_id,
            pipeline_id,
        } => match client
            .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
            .await
        {
            Ok(jobs) => {
                let jobs = jobs
                    .into_iter()
                    .map(|job| {
                        serde_json::json!({
                            "id": job.id,
                            "name": job.name,
                            "status": job.status,
                            "stage": job.stage,
                            "allow_failure": job.allow_failure,
                            "pipeline_id": job.pipeline_id,
                            "ref": job.ref_name,
                            "web_url": job.web_url,
                            "queued_duration": job.queued_duration,
                            "duration": job.duration,
                            "started_at": job.started_at,
                            "finished_at": job.finished_at,
                            "runner": job.runner.and_then(|runner| runner.description),
                        })
                    })
                    .collect::<Vec<_>>();
                CapabilityResponse {
                    success: true,
                    message: format!("{} jobs fetched for pipeline {}", jobs.len(), pipeline_id),
                    data: Some(serde_json::json!({
                        "project_id": project_id,
                        "pipeline_id": pipeline_id,
                        "job_count": jobs.len(),
                        "jobs": jobs,
                    })),
                }
            }
            Err(error) => err(&format!("pipeline jobs: {}", error)),
        },

        // ── GetCiBottlenecks ─────────────────────────────────────────────────
        AgentIntent::GetCiBottlenecks {
            project_id,
            ref_name,
            limit,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };
            match db
                .ci_job_bottlenecks(project_id, ref_name.as_deref(), limit.unwrap_or(25))
                .await
            {
                Ok(rows) => CapabilityResponse {
                    success: true,
                    message: format!("{} bottlenecks fetched", rows.len()),
                    data: Some(serde_json::json!({
                        "project_id": project_id,
                        "ref_name": ref_name,
                        "limit": limit.unwrap_or(25),
                        "rows": rows,
                    })),
                },
                Err(error) => err(&format!("ci bottlenecks: {}", error)),
            }
        }

        // ── ListAllowedActions ────────────────────────────────────────────────
        AgentIntent::ListAllowedActions => {
            use crate::tui::action_registry::{self, Surface};

            let actions: Vec<serde_json::Value> = action_registry::REGISTRY
                .iter()
                .filter(|entry| entry.surfaces.contains(&Surface::Capability))
                .map(|entry| {
                    let mut contract = entry.contract_json();
                    if let Some(obj) = contract.as_object_mut() {
                        obj.insert(
                            "status".to_string(),
                            serde_json::Value::String(
                                if entry.dry_run {
                                    "dry_run_available"
                                } else {
                                    "active"
                                }
                                .to_string(),
                            ),
                        );
                        obj.insert(
                            "risk".to_string(),
                            serde_json::Value::String(entry.risk_tier.label().to_string()),
                        );
                    }
                    contract
                })
                .collect();
            let action_count = actions.len();
            CapabilityResponse {
                success: true,
                message: format!("{} capabilities available", action_count),
                data: Some(serde_json::json!({"actions": actions})),
            }
        }

        // ── PlanValidation ────────────────────────────────────────────────────
        AgentIntent::PlanValidation {
            project_id,
            test_ids,
            ref_name,
        } => {
            let Ok(db) = crate::state::Db::open().await else {
                return err("database unavailable");
            };

            let _ = project_id; // selector misses are global, not per-project
            let since = chrono::Utc::now()
                .checked_sub_signed(chrono::Duration::hours(24))
                .unwrap_or(chrono::Utc::now())
                .to_rfc3339();
            let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);

            let valid = miss_count == 0;
            CapabilityResponse {
                success: valid,
                message: if valid {
                    format!(
                        "Plan for ref '{}' with {} tests is valid",
                        ref_name,
                        test_ids.len()
                    )
                } else {
                    format!(
                        "Plan invalid: {} unresolved selector miss(es) in last 24h",
                        miss_count
                    )
                },
                data: Some(serde_json::json!({
                    "valid": valid,
                    "test_count": test_ids.len(),
                    "ref_name": ref_name,
                    "selector_misses": miss_count,
                })),
            }
        }
    }
}

fn err(msg: &str) -> CapabilityResponse {
    CapabilityResponse {
        success: false,
        message: msg.to_string(),
        data: None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_branch_capability_grant(
    db: &crate::state::Db,
    intent_type: &str,
    action_id: &str,
    ctx: &CapabilityContext,
    project_id: i64,
    branch_name: &str,
    target_ref: Option<&str>,
    new_sha: Option<&str>,
    payload: serde_json::Value,
) -> anyhow::Result<String> {
    let grant_id = format!("grant-{}", uuid::Uuid::new_v4());
    let ref_name = qualify_branch_ref(branch_name);
    let payload = serde_json::json!({
        "protocol_version": ctx.protocol_version,
        "request_id": ctx.request_id,
        "actor": ctx.actor,
        "compat": ctx.compat,
        "scope": {
            "project_id": project_id,
            "ref_name": ref_name,
            "target_ref": target_ref,
            "new_sha": new_sha,
        },
        "intent_payload": payload,
    });
    let payload = serde_json::to_string(&payload)?;
    let intent_id = db
        .record_capability_intent(crate::state::NewCapabilityIntent {
            request_id: &ctx.request_id,
            intent_type,
            action_id,
            project_id: Some(project_id),
            ref_name: Some(&ref_name),
            target_ref,
            actor: &ctx.actor,
            status: "executed",
            payload: &payload,
        })
        .await?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339();
    db.approve_capability_grant(crate::state::NewCapabilityGrant {
        intent_id,
        grant_id: &grant_id,
        action_id,
        project_id: Some(project_id),
        ref_name: &ref_name,
        new_sha,
        required_grant: "agent_task",
        status: "approved",
        expires_at: &expires_at,
        payload: &payload,
    })
    .await?;
    Ok(grant_id)
}

fn qualify_branch_ref(branch_name: &str) -> String {
    if branch_name.starts_with("refs/") {
        branch_name.to_string()
    } else {
        format!("refs/heads/{branch_name}")
    }
}

fn parse_capability_request(bytes: &[u8]) -> anyhow::Result<ParsedCapabilityRequest> {
    if bytes.len() >= 4 {
        let frame_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if frame_len > 0 && frame_len <= MAX_CAPABILITY_FRAME_BYTES && bytes.len() == frame_len + 4
        {
            let request = serde_json::from_slice::<AgentActionRequest>(&bytes[4..])?;
            return Ok(ParsedCapabilityRequest::Enveloped(Box::new(request)));
        }
    }

    if let Ok(request) = serde_json::from_slice::<AgentActionRequest>(bytes) {
        return Ok(ParsedCapabilityRequest::Enveloped(Box::new(request)));
    }

    let intent = serde_json::from_slice::<AgentIntent>(bytes)?;
    Ok(ParsedCapabilityRequest::Compat(intent))
}

fn validate_capability_request(
    parsed: ParsedCapabilityRequest,
) -> anyhow::Result<(AgentIntent, CapabilityContext)> {
    match parsed {
        ParsedCapabilityRequest::Compat(intent) => Ok((intent, CapabilityContext::compat())),
        ParsedCapabilityRequest::Enveloped(request) => {
            if request.protocol_version != CAPABILITY_PROTOCOL_VERSION {
                anyhow::bail!(
                    "unsupported protocol_version '{}', expected '{}'",
                    request.protocol_version,
                    CAPABILITY_PROTOCOL_VERSION
                );
            }
            if request.request_id.trim().is_empty() {
                anyhow::bail!("request_id is required");
            }
            if request.actor.trim().is_empty() {
                anyhow::bail!("actor is required");
            }
            if request.nonce.trim().is_empty() {
                anyhow::bail!("nonce is required");
            }
            if let Some(expires_at) = &request.expires_at {
                let expiry = chrono::DateTime::parse_from_rfc3339(expires_at)
                    .map_err(|e| anyhow::anyhow!("invalid expires_at: {e}"))?;
                if expiry.with_timezone(&chrono::Utc) <= chrono::Utc::now() {
                    anyhow::bail!("request expired");
                }
            }

            let cache = SEEN_NONCES.get_or_init(|| Mutex::new(HashSet::new()));
            let mut seen = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("nonce cache unavailable"))?;
            let nonce_key = format!("{}:{}", request.actor, request.nonce);
            if !seen.insert(nonce_key) {
                anyhow::bail!("replayed nonce");
            }
            if seen.len() > 4096 {
                seen.clear();
            }

            let request = *request;
            let ctx = CapabilityContext {
                request_id: request.request_id,
                actor: request.actor,
                protocol_version: request.protocol_version,
                compat: false,
            };
            Ok((request.intent, ctx))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_ci_yaml_rejects_unknown_scope() {
        let err = dynamic_ci_yaml("unit; rm -rf /").unwrap_err().to_string();
        assert!(err.contains("unsupported test scope"));
    }

    #[test]
    fn typed_ci_yaml_emits_command_list() {
        let yaml = dynamic_ci_yaml("lint").unwrap();
        assert!(yaml.contains("dynamic-lint-job"));
        assert!(yaml.contains("cargo clippy --all-targets"));
        assert!(yaml.contains("cargo fmt -- --check"));
    }

    #[test]
    fn parses_framed_enveloped_request() {
        let request = AgentActionRequest {
            protocol_version: CAPABILITY_PROTOCOL_VERSION.to_string(),
            request_id: format!("req-{}", uuid::Uuid::new_v4()),
            actor: "agent:test".to_string(),
            nonce: uuid::Uuid::new_v4().to_string(),
            expires_at: Some((chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()),
            project_id: Some(1),
            base_ref: Some("main".to_string()),
            base_sha: Some("abc".to_string()),
            idempotency_key: None,
            budget: None,
            grant: None,
            intent: AgentIntent::ListAllowedActions,
        };
        let body = serde_json::to_vec(&request).unwrap();
        let mut framed = (body.len() as u32).to_be_bytes().to_vec();
        framed.extend(body);
        let parsed = parse_capability_request(&framed).unwrap();
        let (intent, ctx) = validate_capability_request(parsed).unwrap();
        assert!(matches!(intent, AgentIntent::ListAllowedActions));
        assert!(!ctx.compat);
        assert_eq!(ctx.actor, "agent:test");
    }

    #[test]
    fn expired_enveloped_request_is_rejected() {
        let request = AgentActionRequest {
            protocol_version: CAPABILITY_PROTOCOL_VERSION.to_string(),
            request_id: format!("req-{}", uuid::Uuid::new_v4()),
            actor: "agent:test".to_string(),
            nonce: uuid::Uuid::new_v4().to_string(),
            expires_at: Some((chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339()),
            project_id: Some(1),
            base_ref: None,
            base_sha: None,
            idempotency_key: None,
            budget: None,
            grant: None,
            intent: AgentIntent::ListAllowedActions,
        };
        let parsed = ParsedCapabilityRequest::Enveloped(Box::new(request));
        let err = validate_capability_request(parsed).unwrap_err().to_string();
        assert!(err.contains("request expired"));
    }
}
