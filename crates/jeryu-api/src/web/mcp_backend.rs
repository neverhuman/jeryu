use std::sync::Arc;

use serde_json::{Value, json};

use super::{WebState, agent_runs, codegraph, control_plane};

pub(super) struct WebMcpBackend {
    state: Arc<WebState>,
    inner: jeryu_mcp::MemoryBackend,
}

impl WebMcpBackend {
    pub(super) fn new(state: Arc<WebState>) -> Self {
        Self {
            state,
            inner: jeryu_mcp::MemoryBackend::new(),
        }
    }
}

impl jeryu_mcp::ToolBackend for WebMcpBackend {
    fn call(
        &self,
        tool: &str,
        args: Value,
        ctx: &jeryu_mcp::backend::McpCallContext,
    ) -> anyhow::Result<jeryu_mcp::ToolResponse> {
        if let Some(response) = self.call_agent_work(tool, args.clone())? {
            return Ok(response);
        }
        if let Some(response) = self.call_tool_build(tool, args.clone())? {
            return Ok(response);
        }
        if let Some(response) = self.call_control_plane(tool, &args)? {
            return Ok(response);
        }
        if let Some(response) = self.call_live_status_tool(tool, &args)? {
            return Ok(response);
        }
        if is_codegraph_tool(tool) && args.get("repo").and_then(Value::as_str).is_some() {
            return self.call_live_codegraph(tool, args);
        }
        self.inner.call(tool, args, ctx)
    }

    fn list(&self) -> Vec<jeryu_mcp::ToolDescriptor> {
        self.inner.list()
    }
}

impl WebMcpBackend {
    fn call_agent_work(
        &self,
        tool: &str,
        args: Value,
    ) -> anyhow::Result<Option<jeryu_mcp::ToolResponse>> {
        let result = match tool {
            "agent_work.start" => agent_runs::mcp_start(self.state.clone(), args),
            "agent_work.status" => agent_runs::mcp_status(&self.state, &args),
            "agent_work.control" => agent_runs::mcp_control(&self.state, args),
            "agent_work.events" => agent_runs::mcp_events(&self.state, &args),
            "agent_work.tail" => agent_runs::mcp_tail(&self.state, &args),
            "agent_work.export_pr" => agent_runs::mcp_export_pr(&self.state, args),
            _ => return Ok(None),
        };
        Ok(Some(match result {
            Ok(value) => jeryu_mcp::ToolResponse::ok(tool, value),
            Err(error) => jeryu_mcp::ToolResponse::error(error),
        }))
    }

    fn call_live_codegraph(
        &self,
        tool: &str,
        args: Value,
    ) -> anyhow::Result<jeryu_mcp::ToolResponse> {
        let repo = args
            .get("repo")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("{tool} requires repo"))?;
        match tool {
            "codegraph.query" => {
                let query = codegraph_query_from_mcp_args(&args);
                match codegraph::query_pack_for_repo(&self.state, repo, query) {
                    Ok(pack) => Ok(jeryu_mcp::ToolResponse::ok(
                        "codegraph impact pack",
                        serde_json::to_value(pack)?,
                    )),
                    Err(error) => Ok(jeryu_mcp::ToolResponse::error(error.to_string())),
                }
            }
            "code.symbols.search" => {
                let graph = live_graph(&self.state)?;
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
                Ok(jeryu_mcp::ToolResponse::ok(
                    "code symbols",
                    json!({
                        "repo": repo,
                        "symbols": graph.search_symbols(
                            args.get("query").and_then(Value::as_str).unwrap_or_default(),
                            limit,
                        ),
                    }),
                ))
            }
            "code.definition" => {
                let graph = live_graph(&self.state)?;
                let symbol = args
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Ok(jeryu_mcp::ToolResponse::ok(
                    "code definition",
                    json!({
                        "repo": repo,
                        "symbol": symbol,
                        "definition": graph.definition(symbol),
                    }),
                ))
            }
            "code.impact" => {
                let query = codegraph_query_from_mcp_args(&args);
                match codegraph::query_pack_for_repo(&self.state, repo, query) {
                    Ok(pack) => Ok(jeryu_mcp::ToolResponse::ok(
                        "code impact",
                        json!({
                            "changed_crates": pack.changed_crates,
                            "affected_crates": pack.affected_crates,
                            "affected_symbols": pack.affected_symbols,
                        }),
                    )),
                    Err(error) => Ok(jeryu_mcp::ToolResponse::error(error.to_string())),
                }
            }
            "code.crate.reverse_deps" => {
                let graph = live_graph(&self.state)?;
                let crate_name = args
                    .get("crate_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Ok(jeryu_mcp::ToolResponse::ok(
                    "crate reverse dependencies",
                    json!({
                        "repo": repo,
                        "crate_name": crate_name,
                        "reverse_deps": graph.reverse_deps(crate_name),
                    }),
                ))
            }
            "code.references" => {
                let graph = live_graph(&self.state)?;
                let symbol = args
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Ok(jeryu_mcp::ToolResponse::ok(
                    "code references",
                    json!({
                        "repo": repo,
                        "symbol": symbol,
                        "references": graph.references(symbol),
                    }),
                ))
            }
            _ => Ok(jeryu_mcp::ToolResponse::error(format!(
                "unknown live codegraph tool: {tool}"
            ))),
        }
    }

    fn call_tool_build(
        &self,
        tool: &str,
        args: Value,
    ) -> anyhow::Result<Option<jeryu_mcp::ToolResponse>> {
        let repo = args.get("repo").and_then(Value::as_str);
        let response = match tool {
            "codegraph.tool_build.status" => {
                let (cluster_count, ignored_count) =
                    self.state.codegraph_store.tool_build_cluster_counts(repo)?;
                jeryu_mcp::ToolResponse::ok(
                    "tool-build status",
                    json!({
                        "repo": repo,
                        "ready": true,
                        "cluster_count": cluster_count,
                        "ignored_count": ignored_count,
                        "schema_version": "codegraph.tool_build/v1",
                    }),
                )
            }
            "codegraph.tool_build.clusters" => {
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
                let include_ignored = args
                    .get("include_ignored")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let clusters =
                    self.state
                        .codegraph_store
                        .tool_build_clusters(repo, limit, include_ignored)?;
                jeryu_mcp::ToolResponse::ok(
                    "tool-build clusters",
                    json!({
                        "repo": repo,
                        "include_ignored": include_ignored,
                        "clusters": clusters,
                    }),
                )
            }
            "codegraph.tool_build.feedback" => {
                let cluster_id = args
                    .get("cluster_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("cluster_id is required"))?;
                let reason = args
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("reason is required"))?;
                let ignored_by = args
                    .get("ignored_by")
                    .and_then(Value::as_str)
                    .unwrap_or("mcp");
                let ignored = self
                    .state
                    .codegraph_store
                    .ignore_tool_build_cluster(cluster_id, reason, ignored_by)?;
                jeryu_mcp::ToolResponse::ok("tool-build feedback recorded", json!(ignored))
            }
            _ => return Ok(None),
        };
        Ok(Some(response))
    }

    fn call_control_plane(
        &self,
        tool: &str,
        args: &Value,
    ) -> anyhow::Result<Option<jeryu_mcp::ToolResponse>> {
        let response = match tool {
            "control_plane.status" => jeryu_mcp::ToolResponse::ok(
                "control-plane status",
                control_plane::mcp_status(&self.state),
            ),
            "control_plane.priorities" => jeryu_mcp::ToolResponse::ok(
                "control-plane priorities",
                control_plane::mcp_priorities(&self.state, args),
            ),
            "repo_graph.clusters" => jeryu_mcp::ToolResponse::ok(
                "repo graph clusters",
                control_plane::mcp_repo_graph_clusters(&self.state, args),
            ),
            "repo_graph.query" => jeryu_mcp::ToolResponse::ok(
                "repo graph",
                control_plane::mcp_repo_graph_query(&self.state, args),
            ),
            "remote.status" => {
                jeryu_mcp::ToolResponse::ok("remote status", control_plane::mcp_remote_status())
            }
            "artifacts.latest" => jeryu_mcp::ToolResponse::ok(
                "latest artifacts",
                control_plane::mcp_artifacts_latest(&self.state),
            ),
            "runner_fabric.status" => jeryu_mcp::ToolResponse::ok(
                "runner fabric status",
                control_plane::mcp_runner_fabric_status(&self.state),
            ),
            _ => return Ok(None),
        };
        Ok(Some(response))
    }

    fn call_live_status_tool(
        &self,
        tool: &str,
        args: &Value,
    ) -> anyhow::Result<Option<jeryu_mcp::ToolResponse>> {
        let response = match tool {
            "get_system_snapshot" => jeryu_mcp::ToolResponse::ok(
                "system snapshot",
                control_plane::mcp_status(&self.state),
            ),
            "get_ci_run_jobs" => jeryu_mcp::ToolResponse::ok(
                "ci run jobs",
                control_plane::mcp_ci_run_jobs(&self.state, args),
            ),
            "get_ci_bottlenecks" => jeryu_mcp::ToolResponse::ok(
                "ci bottlenecks",
                control_plane::mcp_ci_bottlenecks(&self.state, args),
            ),
            "explain_blockers" => jeryu_mcp::ToolResponse::ok(
                "blockers",
                control_plane::mcp_explain_blockers(&self.state, args),
            ),
            "plan_validation" => jeryu_mcp::ToolResponse::ok(
                "validation plan",
                control_plane::mcp_plan_validation(&self.state, args),
            ),
            _ => return Ok(None),
        };
        Ok(Some(response))
    }
}

fn live_graph(state: &Arc<WebState>) -> anyhow::Result<jeryu_codegraph::CodeGraph> {
    Ok(jeryu_codegraph::CodeGraph::from_snapshot(
        state.codegraph_store.load_snapshot()?,
    ))
}

fn codegraph_query_from_mcp_args(args: &Value) -> jeryu_codegraph::CodeGraphQuery {
    jeryu_codegraph::CodeGraphQuery {
        ref_name: args
            .get("ref")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string(),
        changed_paths: string_array(args.get("changed_paths").unwrap_or(&Value::Null)),
        intent: args
            .get("intent")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        question: args
            .get("question")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        max_tokens: args
            .get("max_tokens")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
    }
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_codegraph_tool(tool: &str) -> bool {
    matches!(
        tool,
        "code.symbols.search"
            | "code.definition"
            | "code.impact"
            | "code.crate.reverse_deps"
            | "code.references"
            | "codegraph.query"
    )
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use jeryu_codegraph::{
        CrateDepRow, GraphSnapshot, SymbolRefRow, SymbolRow, ToolBuildScanConfig,
        scan_tool_build_clusters,
    };
    use jeryu_core::ForgeCore;
    use jeryu_mcp::ToolBackend;
    use jeryu_mcp::backend::McpCallContext;
    use jeryu_runnerd::{HoldFailedTreeRequest, StartupSync, WorkcellClaimRequest};
    use serde_json::{Value, json};
    use tempfile::tempdir;

    use super::{WebMcpBackend, WebState};

    fn ctx() -> McpCallContext {
        McpCallContext::mcp("req-test", "tester", jeryu_mcp::MCP_PROTOCOL_VERSION)
    }

    fn write_script(root: &Path, name: &str, body: &str) -> PathBuf {
        let script = root.join(name);
        std::fs::write(&script, body).expect("write script");
        let mut permissions = std::fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod script");
        script
    }

    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixture parent");
        }
        std::fs::write(path, contents).expect("write fixture file");
    }

    fn seed_repairing_workcell(
        state: &Arc<WebState>,
        workspace_root: &Path,
        repo_root: &Path,
    ) -> (String, u64) {
        let claim = WorkcellClaimRequest {
            agent_id: "mcp-agent".to_string(),
            workspace_root: workspace_root.to_path_buf(),
            repo_roots: vec![repo_root.to_path_buf()],
            branch_budget: 1,
            runner_id: "mcp-runner".to_string(),
            runner_epoch: 88,
            git_status_summary: "clean".to_string(),
            ci_snapshot_age_ms: Some(1),
            startup: StartupSync::Rebased {
                main_ref: "refs/heads/main".to_string(),
                base_sha: "base".to_string(),
                head_sha: "head".to_string(),
            },
        };
        let mut manager = state.workcells.lock().expect("workcell manager");
        let held = manager
            .hold_failed_tree(HoldFailedTreeRequest {
                claim,
                ci_run_id: "ci-mcp".to_string(),
                failed_run_id: "failed-mcp".to_string(),
                failed_receipt_id: "receipt-mcp".to_string(),
                failure_log_digest: "sha256:mcp".to_string(),
            })
            .expect("hold failed tree");
        let repairing = manager
            .begin_live_repair(&held.workcell_id, held.runner_epoch)
            .expect("begin live repair");
        (repairing.workcell_id, repairing.runner_epoch)
    }

    fn call(backend: &WebMcpBackend, tool: &str, args: Value) -> jeryu_mcp::ToolResponse {
        backend.call(tool, args, &ctx()).expect("mcp call")
    }

    #[test]
    fn live_agent_work_tools_route_to_agent_run_store() {
        let state = Arc::new(WebState::new(ForgeCore::new()));
        let workspace = tempdir().expect("workspace");
        let repo_root = workspace.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("repo root");
        let script = write_script(
            &repo_root,
            "agent.sh",
            "#!/bin/sh\necho mcp-agent-out\nsleep 0.1\necho mcp-agent-err >&2\n",
        );
        let (workcell_id, runner_epoch) =
            seed_repairing_workcell(&state, workspace.path(), &repo_root);
        let backend = WebMcpBackend::new(state);

        let started = call(
            &backend,
            "agent_work.start",
            json!({
                "source": {
                    "kind": "workcell",
                    "workcell_id": workcell_id,
                    "runner_epoch": runner_epoch
                },
                "io_mode": "pipe",
                "repo_root": repo_root,
                "program": script,
                "budget": {
                    "wall_secs": 5,
                    "output_bytes": 4096
                },
                "require_cgroup": false
            }),
        );
        assert!(started.success, "{started:?}");
        let agent_run_id = started.data.as_ref().unwrap()["agent_run_id"]
            .as_str()
            .expect("agent run id")
            .to_string();

        let status = (0..100)
            .map(|_| {
                let response = call(
                    &backend,
                    "agent_work.status",
                    json!({ "agent_run_id": agent_run_id }),
                );
                if response.data.as_ref().unwrap()["state"] == "running" {
                    std::thread::sleep(Duration::from_millis(20));
                }
                response
            })
            .find(|response| response.data.as_ref().unwrap()["state"] != "running")
            .expect("terminal agent status");
        assert!(status.success, "{status:?}");
        assert_eq!(status.data.as_ref().unwrap()["state"], "succeeded");

        let events = (0..100)
            .map(|_| {
                let response = call(
                    &backend,
                    "agent_work.events",
                    json!({
                        "agent_run_id": agent_run_id,
                        "after_seq": 0,
                        "limit": 20
                    }),
                );
                let has_stdout = response
                    .data
                    .as_ref()
                    .and_then(|data| data["events"].as_array())
                    .is_some_and(|events| {
                        events.iter().any(|event| {
                            event["stream"] == "stdout"
                                && event["text"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .contains("mcp-agent-out")
                        })
                    });
                if !has_stdout {
                    std::thread::sleep(Duration::from_millis(20));
                }
                response
            })
            .find(|response| {
                response
                    .data
                    .as_ref()
                    .and_then(|data| data["events"].as_array())
                    .is_some_and(|events| {
                        events.iter().any(|event| {
                            event["stream"] == "stdout"
                                && event["text"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .contains("mcp-agent-out")
                        })
                    })
            })
            .expect("agent stdout event");
        assert!(events.success, "{events:?}");
        let event_rows = events.data.as_ref().unwrap()["events"]
            .as_array()
            .expect("events");
        assert!(event_rows.iter().any(|event| {
            event["stream"] == "stdout"
                && event["text"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mcp-agent-out")
        }));

        let control = call(
            &backend,
            "agent_work.control",
            json!({
                "agent_run_id": agent_run_id,
                "command": { "kind": "terminate" }
            }),
        );
        assert!(!control.success);

        let export = call(
            &backend,
            "agent_work.export_pr",
            json!({
                "agent_run_id": agent_run_id,
                "owner": "missing",
                "repo": "missing",
                "author": "tester",
                "title": "Export MCP run"
            }),
        );
        assert!(!export.success);
    }

    #[test]
    fn agent_work_tail_streams_raw_tty_for_member_and_denies_non_member() {
        use base64::Engine;

        let state = Arc::new(WebState::new(ForgeCore::new()));
        state.agent_runs.seed_test_run("ar-mcp-tail", 16);
        let raw = [0xff_u8, 0x10, 0x00, b'o', b'k', 0x9c];
        state.agent_runs.push_test_tty(
            "ar-mcp-tail",
            super::agent_runs::test_raw_tty_event("ar-mcp-tail", 1, b"first"),
        );
        state.agent_runs.push_test_tty(
            "ar-mcp-tail",
            super::agent_runs::test_raw_tty_event("ar-mcp-tail", 2, &raw),
        );
        let backend = WebMcpBackend::new(state);

        // Authorized scope: an existing run id streams its raw tty bytes.
        let tail = call(
            &backend,
            "agent_work.tail",
            json!({ "agent_run_id": "ar-mcp-tail", "after_seq": 1 }),
        );
        assert!(tail.success, "{tail:?}");
        let data = tail.data.as_ref().expect("tail data");
        assert_eq!(data["lagged"], false);
        assert_eq!(data["next_after_seq"], 2);
        let events = data["events"].as_array().expect("tail events");
        assert_eq!(events.len(), 1, "only events strictly after the cursor");
        let encoded = events[0]["bytes_b64"].as_str().expect("bytes_b64");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("decode");
        assert_eq!(decoded, raw, "raw non-UTF8 bytes survive the MCP tool path");

        // Non-member / unknown scope: the tool is denied with no payload.
        let denied = call(
            &backend,
            "agent_work.tail",
            json!({ "agent_run_id": "ar-not-a-member" }),
        );
        assert!(!denied.success, "unknown run is denied: {denied:?}");
    }

    #[test]
    fn live_codegraph_tools_use_store_when_repo_is_supplied() {
        let state = Arc::new(WebState::new(ForgeCore::new()));
        state
            .codegraph_store
            .persist(&GraphSnapshot {
                symbols: vec![
                    SymbolRow {
                        crate_name: "jeryu-codegraph".to_string(),
                        file: "crates/jeryu-codegraph/src/lib.rs".to_string(),
                        symbol: "CodeGraph".to_string(),
                        kind: "public".to_string(),
                        is_public: true,
                        line: 7,
                    },
                    SymbolRow {
                        crate_name: "jeryu-mcp".to_string(),
                        file: "crates/jeryu-mcp/src/backend/memory.rs".to_string(),
                        symbol: "MemoryBackend".to_string(),
                        kind: "public".to_string(),
                        is_public: true,
                        line: 11,
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
                    ref_line: 12,
                    ref_kind: "type".to_string(),
                }],
                ..Default::default()
            })
            .expect("persist codegraph snapshot");
        let tool_root = tempdir().expect("tool-build fixture");
        let repeated = r#"
pub fn alpha(input: &str) -> Result<String, String> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let response = call_remote(input);
        if response.is_ok() {
            return response;
        }
        if attempts > 3 {
            return Err("failed".to_string());
        }
    }
}
"#;
        write_file(tool_root.path(), "crates/a/src/lib.rs", repeated);
        write_file(
            tool_root.path(),
            "crates/b/src/lib.rs",
            &repeated.replace("alpha", "beta"),
        );
        let report = scan_tool_build_clusters(
            tool_root.path(),
            "local/repo",
            "commit-a",
            ToolBuildScanConfig {
                window_lines: 5,
                min_normalized_tokens: 12,
                min_occurrences: 2,
                max_file_bytes: 64 * 1024,
                max_clusters: 10,
            },
        )
        .expect("scan tool-build clusters");
        assert!(!report.clusters.is_empty());
        state
            .codegraph_store
            .persist_tool_build_report(&report)
            .expect("persist tool-build report");
        let backend = WebMcpBackend::new(state);

        let symbols = call(
            &backend,
            "code.symbols.search",
            json!({ "repo": "local/repo", "query": "Code", "limit": 5 }),
        );
        assert!(symbols.success, "{symbols:?}");
        assert_eq!(symbols.data.as_ref().unwrap()["repo"], "local/repo");
        assert_eq!(
            symbols.data.as_ref().unwrap()["symbols"][0]["symbol"],
            "CodeGraph"
        );

        let definition = call(
            &backend,
            "code.definition",
            json!({ "repo": "local/repo", "symbol": "CodeGraph" }),
        );
        assert!(definition.success, "{definition:?}");
        assert_eq!(
            definition.data.as_ref().unwrap()["definition"]["file"],
            "crates/jeryu-codegraph/src/lib.rs"
        );

        let reverse_deps = call(
            &backend,
            "code.crate.reverse_deps",
            json!({ "repo": "local/repo", "crate_name": "jeryu-codegraph" }),
        );
        assert!(reverse_deps.success, "{reverse_deps:?}");
        assert_eq!(
            reverse_deps.data.as_ref().unwrap()["reverse_deps"],
            json!(["jeryu-mcp"])
        );

        let references = call(
            &backend,
            "code.references",
            json!({ "repo": "local/repo", "symbol": "CodeGraph" }),
        );
        assert!(references.success, "{references:?}");
        assert_eq!(
            references.data.as_ref().unwrap()["references"][0]["ref_file"],
            "crates/jeryu-mcp/src/backend/memory.rs"
        );

        let query = call(
            &backend,
            "codegraph.query",
            json!({
                "repo": "missing-repo",
                "ref": "main",
                "changed_paths": ["crates/jeryu-codegraph/src/lib.rs"]
            }),
        );
        assert!(!query.success);

        let fallback = call(
            &backend,
            "code.definition",
            json!({ "symbol": "CodeGraph" }),
        );
        assert!(fallback.success, "{fallback:?}");

        let tool_status = call(
            &backend,
            "codegraph.tool_build.status",
            json!({ "repo": "local/repo" }),
        );
        assert!(tool_status.success, "{tool_status:?}");
        assert!(
            tool_status.data.as_ref().unwrap()["cluster_count"]
                .as_u64()
                .unwrap()
                > 0
        );

        let tool_clusters = call(
            &backend,
            "codegraph.tool_build.clusters",
            json!({ "repo": "local/repo", "limit": 10 }),
        );
        assert!(tool_clusters.success, "{tool_clusters:?}");
        let cluster_id = tool_clusters.data.as_ref().unwrap()["clusters"][0]["cluster_id"]
            .as_str()
            .expect("cluster id")
            .to_string();

        let feedback = call(
            &backend,
            "codegraph.tool_build.feedback",
            json!({
                "repo": "local/repo",
                "cluster_id": cluster_id,
                "reason": "fixture boilerplate",
                "ignored_by": "test"
            }),
        );
        assert!(feedback.success, "{feedback:?}");
        assert_eq!(
            feedback.data.as_ref().unwrap()["reason"],
            "fixture boilerplate"
        );
    }
}
