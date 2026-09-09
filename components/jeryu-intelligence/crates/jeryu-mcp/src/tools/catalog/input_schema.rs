//! JSON Schema construction for each tool's `inputSchema`.

use serde_json::Value;

use crate::tools::schema::*;

pub(super) fn tool_input_schema(action_id: &str) -> Option<Value> {
    let schema = match action_id {
        "fetch_capsule" => object_schema(&["job_id"], &[("job_id", integer_schema())]),
        "get_system_snapshot" => object_schema(&[], &[]),
        "get_ci_run_jobs" => object_schema(
            &["repo", "ci_run_id"],
            &[("repo", integer_schema()), ("ci_run_id", integer_schema())],
        ),
        "get_ci_bottlenecks" => object_schema(
            &["repo"],
            &[
                ("repo", integer_schema()),
                ("ref_name", string_schema()),
                ("limit", integer_schema()),
            ],
        ),
        "explain_blockers" => object_schema(
            &["entity_type", "entity_id"],
            &[
                ("entity_type", string_schema()),
                ("entity_id", integer_schema()),
            ],
        ),
        "plan_validation" => object_schema(
            &["repo", "test_ids", "ref_name"],
            &[
                ("repo", integer_schema()),
                ("test_ids", array_schema(string_schema())),
                ("ref_name", string_schema()),
            ],
        ),
        "run_tests" => object_schema(
            &["repo", "target_ref", "test_scope"],
            &[
                ("repo", integer_schema()),
                ("target_ref", string_schema()),
                (
                    "test_scope",
                    enum_schema(&["unit", "integration", "lint", "full"]),
                ),
            ],
        ),
        "propose_patch" => object_schema(
            &[
                "repo",
                "branch_name",
                "base_ref",
                "commit_message",
                "modifications",
            ],
            &[
                ("repo", integer_schema()),
                ("branch_name", string_schema()),
                ("base_ref", string_schema()),
                ("commit_message", string_schema()),
                (
                    "modifications",
                    array_schema(object_schema(
                        &["file_path", "content"],
                        &[("file_path", string_schema()), ("content", string_schema())],
                    )),
                ),
                ("pr_title", string_schema()),
            ],
        ),
        "race_patches" => object_schema(
            &["repo", "base_branch", "commit_message", "hypotheses"],
            &[
                ("repo", integer_schema()),
                ("base_branch", string_schema()),
                ("commit_message", string_schema()),
                (
                    "hypotheses",
                    array_schema(object_schema(
                        &["branch_suffix", "modifications"],
                        &[
                            ("branch_suffix", string_schema()),
                            (
                                "modifications",
                                array_schema(object_schema(
                                    &["file_path", "content"],
                                    &[("file_path", string_schema()), ("content", string_schema())],
                                )),
                            ),
                        ],
                    )),
                ),
            ],
        ),
        "request_merge" => object_schema(
            &["repo", "pr_number", "source_branch", "target_branch"],
            &[
                ("repo", integer_schema()),
                ("pr_number", integer_schema()),
                ("source_branch", string_schema()),
                ("target_branch", string_schema()),
            ],
        ),
        "bug_submit" => object_schema(
            &["report"],
            &[
                ("report", serde_json::json!({"type": "object"})),
                ("idempotency_key", string_schema()),
            ],
        ),
        "bug_list" => object_schema(
            &[],
            &[
                ("project", string_schema()),
                ("status", string_schema()),
                ("sort", string_schema()),
            ],
        ),
        "bug_show" => object_schema(&["bug_id"], &[("bug_id", string_schema())]),
        "bug_ready" => object_schema(&[], &[("project", string_schema())]),
        "bug_update" => object_schema(
            &["bug_id"],
            &[
                ("bug_id", string_schema()),
                ("status", string_schema()),
                ("severity", string_schema()),
                ("priority", string_schema()),
                ("component", string_schema()),
                ("owner", string_schema()),
            ],
        ),
        "bug_record_attempt" => object_schema(
            &["bug_id", "attempt"],
            &[
                ("bug_id", string_schema()),
                ("attempt", serde_json::json!({"type": "object"})),
            ],
        ),
        "workcell.claim" => object_schema(
            &[
                "agent_id",
                "workspace_root",
                "repo_roots",
                "branch_budget",
                "runner_id",
                "runner_epoch",
                "git_status_summary",
                "startup",
            ],
            &[
                ("agent_id", string_schema()),
                ("workspace_root", string_schema()),
                ("repo_roots", array_schema(string_schema())),
                ("branch_budget", integer_schema()),
                ("runner_id", string_schema()),
                ("runner_epoch", integer_schema()),
                ("git_status_summary", string_schema()),
                ("ci_snapshot_age_ms", integer_schema()),
                (
                    "startup",
                    object_schema(
                        &["state", "main_ref", "base_sha", "head_sha"],
                        &[
                            ("state", enum_schema(&["rebased", "failed"])),
                            ("main_ref", string_schema()),
                            ("base_sha", string_schema()),
                            ("head_sha", string_schema()),
                            ("reason", string_schema()),
                        ],
                    ),
                ),
            ],
        ),
        "workcell.status" => object_schema(&["workcell_id"], &[("workcell_id", string_schema())]),
        "workcell.repair_live" => object_schema(
            &[
                "agent_id",
                "workspace_root",
                "repo_roots",
                "branch_budget",
                "runner_id",
                "runner_epoch",
                "git_status_summary",
                "startup",
                "failed_run_id",
                "failed_receipt_id",
                "failure_log_digest",
            ],
            &[
                ("agent_id", string_schema()),
                ("workspace_root", string_schema()),
                ("repo_roots", array_schema(string_schema())),
                ("branch_budget", integer_schema()),
                ("runner_id", string_schema()),
                ("runner_epoch", integer_schema()),
                ("git_status_summary", string_schema()),
                ("ci_snapshot_age_ms", integer_schema()),
                (
                    "startup",
                    object_schema(
                        &["state", "main_ref", "base_sha", "head_sha"],
                        &[
                            ("state", enum_schema(&["rebased", "failed"])),
                            ("main_ref", string_schema()),
                            ("base_sha", string_schema()),
                            ("head_sha", string_schema()),
                            ("reason", string_schema()),
                        ],
                    ),
                ),
                ("failed_run_id", string_schema()),
                ("failed_receipt_id", string_schema()),
                ("failure_log_digest", string_schema()),
            ],
        ),
        "workcell.export_pr" => object_schema(
            &[
                "workcell_id",
                "runner_epoch",
                "branch_suffix",
                "owner",
                "repo",
                "author",
            ],
            &[
                ("workcell_id", string_schema()),
                ("runner_epoch", integer_schema()),
                ("branch_suffix", string_schema()),
                ("owner", string_schema()),
                ("repo", string_schema()),
                ("author", string_schema()),
                ("target_branch", string_schema()),
                ("title", string_schema()),
                ("body", string_schema()),
            ],
        ),
        "workcell.release" => object_schema(
            &["workcell_id", "runner_epoch"],
            &[
                ("workcell_id", string_schema()),
                ("runner_epoch", integer_schema()),
            ],
        ),
        "agent_work.start" => object_schema(
            &["source", "program"],
            &[
                ("source", serde_json::json!({"type": "object"})),
                ("io_mode", enum_schema(&["pty", "pipe"])),
                ("repo_root", string_schema()),
                ("program", string_schema()),
                ("args", array_schema(string_schema())),
                ("env", serde_json::json!({"type": "object"})),
                ("prompt", string_schema()),
                ("budget", serde_json::json!({"type": "object"})),
                ("require_cgroup", serde_json::json!({"type": "boolean"})),
            ],
        ),
        "agent_work.status" => {
            object_schema(&["agent_run_id"], &[("agent_run_id", string_schema())])
        }
        "agent_work.control" => object_schema(
            &["agent_run_id", "command"],
            &[
                ("agent_run_id", string_schema()),
                ("command", serde_json::json!({"type": "object"})),
            ],
        ),
        "agent_work.events" => object_schema(
            &["agent_run_id"],
            &[
                ("agent_run_id", string_schema()),
                ("after_seq", integer_schema()),
                ("limit", integer_schema()),
            ],
        ),
        "agent_work.export_pr" => object_schema(
            &["agent_run_id", "owner", "repo", "author", "title"],
            &[
                ("agent_run_id", string_schema()),
                ("owner", string_schema()),
                ("repo", string_schema()),
                ("author", string_schema()),
                ("branch_suffix", string_schema()),
                ("target_branch", string_schema()),
                ("title", string_schema()),
                ("body", string_schema()),
            ],
        ),
        "code.symbols.search" => object_schema(
            &["query"],
            &[
                ("query", string_schema()),
                ("repo", string_schema()),
                ("limit", integer_schema()),
            ],
        ),
        "code.definition" => object_schema(
            &["symbol"],
            &[("symbol", string_schema()), ("repo", string_schema())],
        ),
        "code.impact" => object_schema(
            &["changed_paths"],
            &[
                ("changed_paths", array_schema(string_schema())),
                ("repo", string_schema()),
            ],
        ),
        "code.crate.reverse_deps" => object_schema(
            &["crate_name"],
            &[("crate_name", string_schema()), ("repo", string_schema())],
        ),
        "code.references" => object_schema(
            &["symbol"],
            &[("symbol", string_schema()), ("repo", string_schema())],
        ),
        "codegraph.query" => object_schema(
            &[],
            &[
                ("repo", string_schema()),
                ("ref", string_schema()),
                ("changed_paths", array_schema(string_schema())),
                ("intent", string_schema()),
                ("question", string_schema()),
                ("max_tokens", integer_schema()),
            ],
        ),
        "codegraph.tool_build.status" => object_schema(&[], &[("repo", string_schema())]),
        "codegraph.tool_build.clusters" => object_schema(
            &[],
            &[
                ("repo", string_schema()),
                ("limit", integer_schema()),
                ("include_ignored", serde_json::json!({"type": "boolean"})),
            ],
        ),
        "tool_finder.clusters" => object_schema(
            &[],
            &[
                ("repo", string_schema()),
                ("limit", integer_schema()),
                ("include_ignored", serde_json::json!({"type": "boolean"})),
            ],
        ),
        "tool_finder.scan" => object_schema(&[], &[]),
        "tool_finder.dashboard" => object_schema(
            &[],
            &[
                ("limit", integer_schema()),
                ("include_ignored", serde_json::json!({"type": "boolean"})),
            ],
        ),
        "tool_registry.summary" => object_schema(&[], &[]),
        "codegraph.tool_build.feedback" => object_schema(
            &["cluster_id", "reason"],
            &[
                ("cluster_id", string_schema()),
                ("reason", string_schema()),
                ("ignored_by", string_schema()),
            ],
        ),
        "control_plane.status" => object_schema(&[], &[]),
        "control_plane.priorities" => object_schema(&[], &[("limit", integer_schema())]),
        "repo_graph.clusters" => object_schema(
            &[],
            &[
                ("cluster_kind", string_schema()),
                ("limit", integer_schema()),
            ],
        ),
        "repo_graph.query" => object_schema(
            &[],
            &[
                ("repo", string_schema()),
                ("cluster_kind", string_schema()),
                ("query", string_schema()),
                ("limit", integer_schema()),
            ],
        ),
        "remote.status" => object_schema(&[], &[("remote", string_schema())]),
        "artifacts.latest" => object_schema(&[], &[("repo", string_schema())]),
        "runner_fabric.status" => object_schema(&[], &[]),
        _ => return None,
    };
    Some(schema)
}
