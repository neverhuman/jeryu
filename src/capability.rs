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
    bridge_mode: bool,
}

impl CapabilityContext {
    pub(crate) fn bridge() -> Self {
        Self {
            request_id: format!("bridge-{}", uuid::Uuid::new_v4()),
            actor: "bridge-capability-client".to_string(),
            protocol_version: "bridge-agent-intent".to_string(),
            bridge_mode: true,
        }
    }

    pub(crate) fn mcp(request_id: String, actor: String, protocol_version: String) -> Self {
        Self {
            request_id,
            actor,
            protocol_version,
            bridge_mode: false,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ParsedCapabilityRequest {
    Enveloped(Box<AgentActionRequest>),
    Bridge(AgentIntent),
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

impl CapabilityResponse {
    pub fn error(msg: &str) -> Self {
        Self {
            success: false,
            message: msg.to_string(),
            data: None,
        }
    }
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
                                    bridge_mode = ctx.bridge_mode,
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

#[path = "capability_actions.rs"]
mod actions;
pub(crate) use actions::*;
