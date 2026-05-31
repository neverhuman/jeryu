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
        _ => return None,
    };
    Some(schema)
}
