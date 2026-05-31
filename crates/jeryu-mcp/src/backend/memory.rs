//! Deterministic in-memory backend for tests.
//!
//! Validates argument shape via the catalog parsers and returns a predictable
//! [`ToolResponse`] per tool. Holds an in-memory bug store.

use std::sync::Mutex;

use serde_json::Value;

use super::{BugStore, McpCallContext, ToolBackend, ToolDescriptor, ToolResponse};

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
                let id = args
                    .get("bug_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
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
                let id = args
                    .get("bug_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
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
                let id = args
                    .get("bug_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match self.record_attempt(id, arg("attempt")) {
                    Ok(record) => ToolResponse::ok("attempt recorded", record),
                    Err(e) => ToolResponse::error(e.to_string()),
                }
            }
            other => ToolResponse::error(format!("unknown tool: {other}")),
        };
        Ok(resp)
    }

    fn list(&self) -> Vec<ToolDescriptor> {
        crate::tools::catalog()
    }
}
