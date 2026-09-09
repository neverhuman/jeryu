//! Deterministic in-memory backend for tests.
//!
//! Validates argument shape via the catalog parsers and returns a predictable
//! [`ToolResponse`] per tool. Holds an in-memory bug store.

use std::sync::Mutex;

use serde_json::Value;

use super::{BugStore, McpCallContext, ToolBackend, ToolDescriptor, ToolResponse};
use jeryu_codegraph::{
    CodeGraph, CodegraphQuery, CrateDepRow, GraphSnapshot, SymbolRefRow, SymbolRow, query_snapshot,
};

/// Deterministic in-memory backend for tests. Validates argument shape via the catalog
/// parsers and returns a predictable `ToolResponse` per tool. Holds an in-memory bug store.
pub struct MemoryBackend {
    bugs: Mutex<Vec<Value>>,
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            bugs: Mutex::new(Vec::new()),
        }
    }
}

impl BugStore for MemoryBackend {
    fn submit(&self, report: Value, idempotency_key: Option<String>) -> anyhow::Result<Value> {
        let mut bugs = self.bugs.lock().expect("bug store lock");
        let id = format!("BUG-{}", bugs.len() + 1);
        let record = serde_json::json!({
            "bug_id": id,
            "report": report,
            "idempotency_key": idempotency_key,
            "attempts": [],
        });
        bugs.push(record.clone());
        Ok(record)
    }

    fn list(
        &self,
        _project: Option<String>,
        _status: Option<String>,
        _sort: Option<String>,
    ) -> anyhow::Result<Value> {
        let bugs = self.bugs.lock().expect("bug store lock");
        Ok(Value::Array(bugs.clone()))
    }

    fn show(&self, bug_id: &str) -> anyhow::Result<Value> {
        let bugs = self.bugs.lock().expect("bug store lock");
        bugs.iter()
            .find(|b| b.get("bug_id").and_then(Value::as_str) == Some(bug_id))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown bug {bug_id}"))
    }

    fn ready(&self, _project: Option<String>) -> anyhow::Result<Value> {
        let bugs = self.bugs.lock().expect("bug store lock");
        Ok(Value::Array(bugs.clone()))
    }

    fn update(
        &self,
        bug_id: &str,
        status: Option<String>,
        severity: Option<String>,
        priority: Option<String>,
        component: Option<String>,
        owner: Option<String>,
    ) -> anyhow::Result<Value> {
        let mut bugs = self.bugs.lock().expect("bug store lock");
        let record = bugs
            .iter_mut()
            .find(|b| b.get("bug_id").and_then(Value::as_str) == Some(bug_id))
            .ok_or_else(|| anyhow::anyhow!("unknown bug {bug_id}"))?;
        let map = record.as_object_mut().expect("bug record object");
        for (k, v) in [
            ("status", status),
            ("severity", severity),
            ("priority", priority),
            ("component", component),
            ("owner", owner),
        ] {
            if let Some(value) = v {
                map.insert(k.to_string(), Value::String(value));
            }
        }
        Ok(record.clone())
    }

    fn record_attempt(&self, bug_id: &str, attempt: Value) -> anyhow::Result<Value> {
        let mut bugs = self.bugs.lock().expect("bug store lock");
        let record = bugs
            .iter_mut()
            .find(|b| b.get("bug_id").and_then(Value::as_str) == Some(bug_id))
            .ok_or_else(|| anyhow::anyhow!("unknown bug {bug_id}"))?;
        record
            .get_mut("attempts")
            .and_then(Value::as_array_mut)
            .expect("attempts array")
            .push(attempt);
        Ok(record.clone())
    }
}

impl ToolBackend for MemoryBackend {
    fn call(&self, tool: &str, args: Value, _ctx: &McpCallContext) -> anyhow::Result<ToolResponse> {
        // The transport already validated the tool exists in the catalog and parsed
        // arguments via `tool_definition(...).build_intent(...)`. Here we produce a
        // deterministic, brandless response per tool family.
        let arg = |k: &str| args.get(k).cloned().unwrap_or(Value::Null);
        let resp = match tool {
            "fetch_capsule" => ToolResponse::ok(
                "fetched capsule",
                serde_json::json!({ "job_id": arg("job_id"), "capsule": Value::Null }),
            ),
            "get_system_snapshot" => ToolResponse::ok(
                "system snapshot",
                serde_json::json!({ "engine_ready": true, "open_prs": 0 }),
            ),
            "get_ci_run_jobs" => ToolResponse::ok(
                "ci run jobs",
                serde_json::json!({ "repo": arg("repo"), "ci_run_id": arg("ci_run_id"), "jobs": [] }),
            ),
            "get_ci_bottlenecks" => ToolResponse::ok(
                "ci bottlenecks",
                serde_json::json!({ "repo": arg("repo"), "bottlenecks": [] }),
            ),
            "explain_blockers" => ToolResponse::ok(
                "no blockers",
                serde_json::json!({
                    "entity_type": arg("entity_type"),
                    "entity_id": arg("entity_id"),
                    "mergeable": true,
                    "blockers": [],
                }),
            ),
            "plan_validation" => ToolResponse::ok(
                "validation plan",
                serde_json::json!({ "lanes": ["unit"], "blockers": [] }),
            ),
            "run_tests" => ToolResponse::ok(
                "ci run triggered",
                serde_json::json!({ "ci_run_id": 1, "scope": arg("test_scope") }),
            ),
            "propose_patch" => ToolResponse::ok(
                "patch proposed",
                serde_json::json!({ "pr_number": 1, "url": "pr://1" }),
            ),
            "race_patches" => {
                ToolResponse::ok("patches racing", serde_json::json!({ "ci_run_ids": [] }))
            }
            "request_merge" => ToolResponse::ok(
                "enqueued to merge queue",
                serde_json::json!({ "pr_number": arg("pr_number"), "enqueued": true }),
            ),
            "bug_submit" => {
                let record = self.submit(
                    arg("report"),
                    args.get("idempotency_key")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                )?;
                ToolResponse::ok("bug submitted", record)
            }
            "bug_list" => {
                let record = BugStore::list(
                    self,
                    args.get("project")
                        .and_then(Value::as_str)
                        .map(String::from),
                    args.get("status").and_then(Value::as_str).map(String::from),
                    args.get("sort").and_then(Value::as_str).map(String::from),
                )?;
                ToolResponse::ok("bugs", record)
            }
            "bug_show" => {
                let id = match args.get("bug_id").and_then(Value::as_str) {
                    Some(id) => id,
                    None => return Ok(ToolResponse::error("bug_show requires bug_id")),
                };
                match self.show(id) {
                    Ok(record) => ToolResponse::ok("bug", record),
                    Err(e) => ToolResponse::error(e.to_string()),
                }
            }
            "bug_ready" => {
                let record = self.ready(
                    args.get("project")
                        .and_then(Value::as_str)
                        .map(String::from),
                )?;
                ToolResponse::ok("ready bugs", record)
            }
            "bug_update" => {
                let id = match args.get("bug_id").and_then(Value::as_str) {
                    Some(id) => id,
                    None => return Ok(ToolResponse::error("bug_update requires bug_id")),
                };
                let pick = |k: &str| args.get(k).and_then(Value::as_str).map(String::from);
                match self.update(
                    id,
                    pick("status"),
                    pick("severity"),
                    pick("priority"),
                    pick("component"),
                    pick("owner"),
                ) {
                    Ok(record) => ToolResponse::ok("bug updated", record),
                    Err(e) => ToolResponse::error(e.to_string()),
                }
            }
            "bug_record_attempt" => {
                let id = match args.get("bug_id").and_then(Value::as_str) {
                    Some(id) => id,
                    None => return Ok(ToolResponse::error("bug_record_attempt requires bug_id")),
                };
                match self.record_attempt(id, arg("attempt")) {
                    Ok(record) => ToolResponse::ok("attempt recorded", record),
                    Err(e) => ToolResponse::error(e.to_string()),
                }
            }
            "workcell.claim" => ToolResponse::ok(
                "workcell claimed",
                serde_json::json!({
                    "workcell_id": format!("wc-{}", arg("agent_id").as_str().unwrap_or("agent")),
                    "state": "claimed",
                    "agent_id": arg("agent_id"),
                    "workspace_root": arg("workspace_root"),
                    "repo_roots": arg("repo_roots"),
                    "branch_budget": arg("branch_budget"),
                    "runner_id": arg("runner_id"),
                    "runner_epoch": arg("runner_epoch"),
                    "git_status_summary": arg("git_status_summary"),
                    "ci_snapshot_age_ms": arg("ci_snapshot_age_ms"),
                    "startup": arg("startup"),
                }),
            ),
            "workcell.status" => ToolResponse::ok(
                "workcell status",
                serde_json::json!({
                    "workcell_id": arg("workcell_id"),
                    "state": "ready",
                }),
            ),
            "workcell.repair_live" => ToolResponse::ok(
                "workcell repair started",
                serde_json::json!({
                    "held": {
                        "workcell_id": format!("wc-{}", arg("agent_id").as_str().unwrap_or("agent")),
                        "state": "held",
                        "agent_id": arg("agent_id"),
                        "workspace_root": arg("workspace_root"),
                        "repo_roots": arg("repo_roots"),
                        "branch_budget": arg("branch_budget"),
                        "runner_id": arg("runner_id"),
                        "runner_epoch": arg("runner_epoch"),
                        "git_status_summary": arg("git_status_summary"),
                        "ci_snapshot_age_ms": arg("ci_snapshot_age_ms"),
                        "startup": arg("startup"),
                        "failed_run_id": arg("failed_run_id"),
                        "failed_receipt_id": arg("failed_receipt_id"),
                        "failure_log_digest": arg("failure_log_digest"),
                    },
                    "repairing": {
                        "workcell_id": format!("wc-{}", arg("agent_id").as_str().unwrap_or("agent")),
                        "state": "repairing",
                    }
                }),
            ),
            "workcell.export_pr" => ToolResponse::ok(
                "repair branch exported",
                serde_json::json!({
                    "workcell_id": arg("workcell_id"),
                    "branch": format!(
                        "agents/{}/workcells/{}/{}",
                        arg("author").as_str().unwrap_or("agent"),
                        arg("workcell_id").as_str().unwrap_or("wc"),
                        arg("branch_suffix").as_str().unwrap_or("repair"),
                    ),
                    "target_branch": arg("target_branch"),
                    "pull_request_number": 1,
                }),
            ),
            "workcell.release" => ToolResponse::ok(
                "workcell released",
                serde_json::json!({
                    "workcell_id": arg("workcell_id"),
                    "released": true,
                }),
            ),
            "agent_work.start" => ToolResponse::ok(
                "agent work started",
                serde_json::json!({
                    "agent_run_id": "ar-memory-000001",
                    "status_url": "/api/v1/agent-runs/ar-memory-000001",
                    "events_url": "/api/v1/agent-runs/ar-memory-000001/events",
                    "control_url": "/api/v1/agent-runs/ar-memory-000001/control",
                    "export_pr_url": "/api/v1/agent-runs/ar-memory-000001/export_pr",
                    "ws_scope": "agent_run.ar-memory-000001",
                    "tty_topic": "jeryu.agent.tty.v1",
                    "control_topic": "jeryu.agent.control.v1",
                    "io_mode": arg("io_mode"),
                    "state": "running",
                }),
            ),
            "agent_work.status" => ToolResponse::ok(
                "agent work status",
                serde_json::json!({
                    "agent_run_id": arg("agent_run_id"),
                    "state": "running",
                    "events": [],
                    "controls": [],
                }),
            ),
            "agent_work.control" => ToolResponse::ok(
                "agent work control accepted",
                serde_json::json!({
                    "agent_run_id": arg("agent_run_id"),
                    "accepted": true,
                    "control_seq": 1,
                    "command": arg("command").get("kind").cloned().unwrap_or(Value::Null),
                }),
            ),
            "agent_work.events" => ToolResponse::ok(
                "agent work events",
                serde_json::json!({
                    "agent_run_id": arg("agent_run_id"),
                    "after_seq": arg("after_seq"),
                    "next_after_seq": arg("after_seq").as_u64().unwrap_or(0),
                    "limit": arg("limit").as_u64().unwrap_or(100),
                    "has_more": false,
                    "events": [],
                    "tty_events": [],
                }),
            ),
            "agent_work.export_pr" => ToolResponse::ok(
                "agent work exported",
                serde_json::json!({
                    "agent_run_id": arg("agent_run_id"),
                    "branch": format!("agents/{}/agent-work", arg("author").as_str().unwrap_or("agent")),
                    "target_branch": arg("target_branch").as_str().unwrap_or("main"),
                    "pull_request_number": 1,
                    "url": format!("/{}/{}/pull/1", arg("owner").as_str().unwrap_or("local"), arg("repo").as_str().unwrap_or("repo")),
                }),
            ),
            "code.symbols.search" => {
                let graph = CodeGraph::from_snapshot(sample_codegraph_snapshot());
                let limit = arg("limit").as_u64().unwrap_or(20) as usize;
                let query = match args.get("query").and_then(Value::as_str) {
                    Some(query) => query,
                    None => return Ok(ToolResponse::error("code.symbols.search requires query")),
                };
                ToolResponse::ok(
                    "code symbols",
                    serde_json::json!({
                        "symbols": graph.search_symbols(query, limit)
                    }),
                )
            }
            "code.definition" => {
                let graph = CodeGraph::from_snapshot(sample_codegraph_snapshot());
                let symbol = match args.get("symbol").and_then(Value::as_str) {
                    Some(symbol) => symbol,
                    None => return Ok(ToolResponse::error("code.definition requires symbol")),
                };
                ToolResponse::ok(
                    "code definition",
                    serde_json::json!({
                        "symbol": symbol.to_string(),
                        "definition": graph.definition(symbol)
                    }),
                )
            }
            "code.impact" => {
                let query = CodegraphQuery {
                    changed_paths: string_array(&arg("changed_paths")),
                    symbol: None,
                    crate_name: None,
                    limit: 20,
                };
                let pack = query_snapshot(sample_codegraph_snapshot(), "2".to_string(), &query);
                ToolResponse::ok("code impact", serde_json::to_value(pack.impact).unwrap())
            }
            "code.crate.reverse_deps" => {
                let graph = CodeGraph::from_snapshot(sample_codegraph_snapshot());
                let crate_name = match args.get("crate_name").and_then(Value::as_str) {
                    Some(crate_name) => crate_name,
                    None => {
                        return Ok(ToolResponse::error(
                            "code.crate.reverse_deps requires crate_name",
                        ));
                    }
                };
                ToolResponse::ok(
                    "crate reverse dependencies",
                    serde_json::json!({
                        "crate_name": crate_name.to_string(),
                        "reverse_deps": graph.reverse_deps(crate_name)
                    }),
                )
            }
            "code.references" => {
                let graph = CodeGraph::from_snapshot(sample_codegraph_snapshot());
                let symbol = match args.get("symbol").and_then(Value::as_str) {
                    Some(symbol) => symbol,
                    None => return Ok(ToolResponse::error("code.references requires symbol")),
                };
                ToolResponse::ok(
                    "code references",
                    serde_json::json!({
                        "symbol": symbol.to_string(),
                        "references": graph.references(symbol)
                    }),
                )
            }
            "codegraph.query" => {
                let query = CodegraphQuery {
                    changed_paths: string_array(&arg("changed_paths")),
                    symbol: optional_string(&arg("symbol")),
                    crate_name: optional_string(&arg("crate_name")),
                    limit: arg("limit").as_u64().unwrap_or(20) as usize,
                };
                ToolResponse::ok(
                    "codegraph impact pack",
                    serde_json::to_value(query_snapshot(
                        sample_codegraph_snapshot(),
                        "2".to_string(),
                        &query,
                    ))?,
                )
            }
            "codegraph.tool_build.status" => ToolResponse::ok(
                "tool-build status",
                serde_json::json!({
                    "repo": arg("repo"),
                    "ready": true,
                    "cluster_count": 1,
                    "ignored_count": 0,
                    "schema_version": "codegraph.tool_build/v1",
                }),
            ),
            "codegraph.tool_build.clusters" => ToolResponse::ok(
                "tool-build clusters",
                serde_json::json!({
                    "repo": arg("repo"),
                    "include_ignored": arg("include_ignored").as_bool().unwrap_or(false),
                    "clusters": [sample_tool_build_cluster()],
                }),
            ),
            "codegraph.tool_build.feedback" => ToolResponse::ok(
                "tool-build feedback recorded",
                serde_json::json!({
                    "cluster_id": arg("cluster_id"),
                    "reason": arg("reason"),
                    "ignored_by": arg("ignored_by"),
                    "ignored_at": "0",
                }),
            ),
            "control_plane.status" => {
                ToolResponse::ok("control-plane status", sample_control_plane_snapshot())
            }
            "control_plane.priorities" => ToolResponse::ok(
                "control-plane priorities",
                serde_json::json!({
                    "priorities": sample_control_plane_snapshot()["priorities"].clone()
                }),
            ),
            "repo_graph.clusters" => ToolResponse::ok(
                "repo graph clusters",
                serde_json::json!({
                    "schemaVersion": "jeryu.repo_graph/v1",
                    "clusters": sample_control_plane_snapshot()["repoGraph"]["clusters"].clone()
                }),
            ),
            "repo_graph.query" => ToolResponse::ok(
                "repo graph",
                sample_control_plane_snapshot()["repoGraph"].clone(),
            ),
            "remote.status" => ToolResponse::ok(
                "remote status",
                sample_control_plane_snapshot()["mirror"].clone(),
            ),
            "artifacts.latest" => ToolResponse::ok(
                "latest artifacts",
                sample_control_plane_snapshot()["artifacts"].clone(),
            ),
            "runner_fabric.status" => ToolResponse::ok(
                "runner fabric status",
                sample_control_plane_snapshot()["runners"].clone(),
            ),
            other => ToolResponse::error(format!("unknown tool: {other}")),
        };
        Ok(resp)
    }

    fn list(&self) -> Vec<ToolDescriptor> {
        crate::tools::catalog()
    }
}

fn string_array(value: &Value) -> Vec<String> {
    match value.as_array() {
        Some(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        None => Vec::new(),
    }
}

fn optional_string(value: &Value) -> Option<String> {
    value.as_str().map(ToString::to_string).or_else(|| {
        value
            .get("Some")
            .and_then(Value::as_str)
            .map(ToString::to_string)
    })
}

fn sample_codegraph_snapshot() -> GraphSnapshot {
    GraphSnapshot {
        symbols: vec![
            SymbolRow {
                crate_name: "jeryu-codegraph".to_string(),
                file: "crates/jeryu-codegraph/src/lib.rs".to_string(),
                symbol: "CodeGraph".to_string(),
                kind: "public".to_string(),
                is_public: true,
                line: 10,
            },
            SymbolRow {
                crate_name: "jeryu-mcp".to_string(),
                file: "crates/jeryu-mcp/src/backend/memory.rs".to_string(),
                symbol: "MemoryBackend".to_string(),
                kind: "public".to_string(),
                is_public: true,
                line: 12,
            },
        ],
        crate_deps: vec![CrateDepRow {
            crate_name: "jeryu-mcp".to_string(),
            depends_on: "jeryu-codegraph".to_string(),
        }],
        symbol_refs: vec![SymbolRefRow {
            crate_name: "jeryu-codegraph".to_string(),
            file: "crates/jeryu-codegraph/src/lib.rs".to_string(),
            symbol: "CodeGraph".to_string(),
            ref_file: "crates/jeryu-mcp/src/backend/memory.rs".to_string(),
            ref_line: 7,
            ref_kind: "type".to_string(),
        }],
        ..Default::default()
    }
}

fn sample_tool_build_cluster() -> Value {
    serde_json::json!({
        "cluster_id": "toolbuild-memory-0001",
        "repo_id": "memory",
        "commit_sha": "memory",
        "fingerprint": "memory",
        "score": 1200,
        "occurrence_count": 3,
        "repo_count": 1,
        "file_count": 2,
        "total_lines": 24,
        "language": "rust",
        "insight": "rust normalized window repeats 3 times across 2 file(s), covering 24 lines; anchors: kw:if, call:unwrap.",
        "normalized_preview": "kw:if id op:= call:some\ncall:unwrap",
        "occurrences": [
            {
                "repo_id": "memory",
                "commit_sha": "memory",
                "path": "crates/jeryu-codegraph/src/tool_build.rs",
                "start_line": 1,
                "end_line": 8,
                "language": "rust",
                "normalized_token_count": 48
            }
        ]
    })
}

fn sample_control_plane_snapshot() -> Value {
    serde_json::json!({
        "schemaVersion": "jeryu.control_plane/v1",
        "generatedAt": "1970-01-01T00:00:00Z",
        "localAuthority": {
            "sourceOfTruth": "local_jeryu",
            "state": "fresh",
            "docsUrl": "docs/architecture.md"
        },
        "summary": {
            "repoCount": 0,
            "openPrCount": 0,
            "draftPrCount": 0,
            "queuedCheckCount": 0,
            "runningCheckCount": 0,
            "failingCheckCount": 0,
            "missingCheckPrCount": 0,
            "priorityCount": 1,
            "criticalPriorityCount": 0,
            "highPriorityCount": 0,
            "mirrorState": "missing",
            "artifactState": "missing",
            "runnerState": "fresh"
        },
        "repos": [],
        "pullRequests": [],
        "checkRuns": [],
        "workflows": [],
        "releases": {
            "state": "missing",
            "latestRelease": null,
            "releaseCount": 0,
            "reason": "memory backend has no durable releases",
            "docsUrl": "docs/release.md"
        },
        "artifacts": {
            "schemaVersion": "jeryu.artifacts.latest/v1",
            "state": "missing",
            "latestBuild": {
                "state": "missing",
                "artifactCount": 0,
                "reason": "memory backend has no artifacts",
                "sourceLinks": []
            },
            "latestRelease": {
                "state": "missing",
                "artifactCount": 0,
                "reason": "memory backend has no release artifacts",
                "sourceLinks": []
            },
            "mirrorArtifacts": {
                "state": "missing",
                "artifactCount": 0,
                "reason": "memory backend has no mirror artifacts",
                "sourceLinks": []
            },
            "docsUrl": "docs/release.md#release-receipt",
            "absenceIsSuccess": false
        },
        "runners": {
            "schemaVersion": "jeryu.runner_fabric/v1",
            "local": {
                "state": "fresh",
                "nodes": 4,
                "onlineRunners": 4,
                "offlineRunners": 0,
                "busyRunners": 0,
                "idleRunners": 40,
                "totalSlots": 40,
                "activeSlots": 40,
                "utilization": 0.0
            },
            "mirror": {
                "name": "github_actions_runners",
                "state": "missing",
                "reason": "mirror adapter unavailable",
                "docsUrl": "docs/agent-native-standard.md"
            }
        },
        "workcells": { "items": [], "summary": null },
        "agentRuns": [],
        "codegraph": {
            "state": "missing",
            "indexedSymbols": 0,
            "indexedReferences": 0,
            "crateEdges": 0,
            "indexedFiles": 0,
            "latestIndexRun": null,
            "reason": "memory backend has no codegraph index"
        },
        "toolBuild": {
            "state": "missing",
            "clusterCount": 0,
            "ignoredCount": 0,
            "topClusters": []
        },
        "mcp": {
            "state": "fresh",
            "toolCount": 42,
            "liveBackedTools": [],
            "degradedTools": []
        },
        "mirror": {
            "schemaVersion": "jeryu.remote.status/v1",
            "state": "missing",
            "mirrors": [{
                "name": "github",
                "state": "missing",
                "reason": "mirror adapter unavailable",
                "docsUrl": "docs/agent-native-standard.md"
            }],
            "divergence": {
                "state": "unknown",
                "reason": "mirror state unavailable",
                "localDefaultBranches": [],
                "mirrorDefaultBranches": []
            }
        },
        "priorities": [{
            "id": "memory-mirror-missing",
            "title": "Mirror evidence unavailable",
            "severity": "medium",
            "score": 600,
            "confidence": 1.0,
            "owner": "forge-api",
            "proofLane": "cargo test -p jeryu-mcp --jobs 40",
            "recommendedAction": "configure a read-only mirror adapter before relying on mirror state",
            "evidence": ["missing mirror evidence is explicit"],
            "sourceLinks": [],
            "state": "missing",
            "rulesVersion": "rules-v1"
        }],
        "repoGraph": {
            "schemaVersion": "jeryu.repo_graph/v1",
            "generatedAt": "1970-01-01T00:00:00Z",
            "nodes": [],
            "edges": [],
            "clusters": [{
                "id": "cluster:superseded-mirror",
                "label": "Mirror evidence",
                "kind": "superseded_mirror",
                "state": "missing",
                "severity": "medium",
                "nodeIds": [],
                "insights": ["mirror state unavailable"]
            }],
            "insights": []
        }
    })
}
