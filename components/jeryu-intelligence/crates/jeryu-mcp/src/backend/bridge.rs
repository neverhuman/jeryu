//! Real jeryu-agentbridge-backed [`ToolBackend`].
//!
//! Where [`MemoryBackend`](super::MemoryBackend) returns deterministic no-ops,
//! `BridgeBackend` routes the mutating MCP tools (`propose_patch`,
//! `request_merge`) through [`jeryu_agentbridge::AgentBridge`] so each call
//! performs a real, scope-validated, bounded mutation: the agent write scope and
//! the proof-engine ownership gate both run, out-of-scope writes are denied, and
//! an append-only receipt is recorded. Read-style tools resolve against the same
//! bridge state.
//!
//! Interior mutability: [`ToolBackend::call`] takes `&self`, but the bridge
//! mutation methods take `&mut self`, so the bridge lives behind a [`Mutex`].
//! The catalog + bug store are delegated to a wrapped [`MemoryBackend`] so this
//! backend stays focused on the agent mutation seam.

use std::sync::Mutex;

use serde_json::Value;

use super::{BugStore, McpCallContext, MemoryBackend, ToolBackend, ToolDescriptor, ToolResponse};

use jeryu_agentbridge::{AgentBridge, FilePatch, ScopedPatchRequest};
use jeryu_core::phase7::{AgentId, AgentScope, ChangedPath, PullRequest, PullRequestId, RepoId};
use jeryu_proof::default_phase7_engine;

/// Default per-call mutation cap when a tool does not constrain it further.
const DEFAULT_MAX_PATHS: usize = 16;

/// Agent-bridge-backed backend. The bridge performs the scope-validated bounded
/// mutations; the embedded [`MemoryBackend`] supplies the tool catalog and the
/// in-memory bug store so this type only owns the agent mutation surface.
pub struct BridgeBackend {
    bridge: Mutex<AgentBridge>,
    /// Path prefixes an MCP-driven agent is allowed to write under. A write that
    /// escapes every prefix is denied by `AgentScope::permits_all` before any
    /// receipt is recorded.
    allowed_paths: Vec<String>,
    /// Delegate for the read/catalog/bug tools that are not agent mutations.
    inner: MemoryBackend,
}

impl Default for BridgeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl BridgeBackend {
    /// Backend with the Phase 7 default proof engine and an unrestricted-prefix
    /// scope (paths are still gated by the proof-engine ownership rules).
    pub fn new() -> Self {
        Self::with_allowed_paths(vec!["crates/".to_string()])
    }

    /// Backend that only permits agent writes under `allowed_paths` prefixes.
    pub fn with_allowed_paths(allowed_paths: Vec<String>) -> Self {
        Self {
            bridge: Mutex::new(AgentBridge::new(default_phase7_engine())),
            allowed_paths,
            inner: MemoryBackend::new(),
        }
    }

    /// Backend over a caller-supplied bridge (lets tests seed PRs / scope).
    pub fn with_bridge(bridge: AgentBridge, allowed_paths: Vec<String>) -> Self {
        Self {
            bridge: Mutex::new(bridge),
            allowed_paths,
            inner: MemoryBackend::new(),
        }
    }

    fn scope(&self, actor: &str, repo: &RepoId) -> AgentScope {
        AgentScope {
            agent: AgentId::new(format!("mcp:{actor}")),
            repo: repo.clone(),
            allowed_paths: self.allowed_paths.clone(),
            max_paths: DEFAULT_MAX_PATHS,
        }
    }

    /// `propose_patch`: synthesize the target PR (if not already present) and run
    /// the bounded, scope-validated mutation through the bridge.
    fn propose_patch(&self, args: &Value, ctx: &McpCallContext) -> ToolResponse {
        let repo = RepoId::new(string_or_int(args, "repo"));
        let branch = str_arg(args, "branch_name");
        let base_ref = str_arg(args, "base_ref");
        let paths: Vec<String> = match args.get("modifications").and_then(Value::as_array) {
            Some(mods) => mods
                .iter()
                .filter_map(|m| m.get("file_path").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect(),
            None => Vec::new(),
        };

        if paths.is_empty() {
            return ToolResponse::error("propose_patch requires at least one modification");
        }

        // Derive a deterministic PR id from repo + branch so repeated proposals
        // for the same branch target the same PR.
        let pr_id = PullRequestId::new(format!("{repo}/{branch}"));
        let head_sha = format!("head-{branch}");

        let pr = PullRequest::new(
            repo.clone(),
            pr_id.clone(),
            branch.clone(),
            base_ref.clone(),
            base_ref.clone(),
            head_sha.clone(),
            paths.iter().map(|p| ChangedPath::new(p.as_str())).collect(),
        );

        let scope = self.scope(&ctx.actor, &repo);
        let request = ScopedPatchRequest {
            scope,
            pr: pr_id.clone(),
            base_sha: head_sha,
            patches: paths
                .iter()
                .map(|p| FilePatch {
                    path: p.clone(),
                    patch: String::new(),
                })
                .collect(),
        };

        let mut bridge = self.bridge.lock().expect("bridge lock");
        bridge.upsert_pr(pr);
        match bridge.apply_scoped_patch(request) {
            Ok(resp) => ToolResponse::ok(
                "patch proposed",
                serde_json::json!({
                    "pr": pr_id.to_string(),
                    "receipt_id": resp.receipt_id.to_string(),
                    "changed_paths": resp.changed_paths,
                }),
            ),
            Err(err) => ToolResponse::error(err.to_string()),
        }
    }

    /// `request_merge`: evaluate the PR's mergeability through the proof gate.
    fn request_merge(&self, args: &Value, _ctx: &McpCallContext) -> ToolResponse {
        let repo = RepoId::new(string_or_int(args, "repo"));
        // Prefer an explicit PR id; otherwise reconstruct it from repo + branch
        // the way `propose_patch` minted it.
        let pr_id = match args.get("pr").and_then(Value::as_str) {
            Some(id) => PullRequestId::new(id),
            None => {
                let branch = str_arg(args, "source_branch");
                PullRequestId::new(format!("{repo}/{branch}"))
            }
        };

        let bridge = self.bridge.lock().expect("bridge lock");
        match bridge.mergeability(&pr_id) {
            Ok(merge) => ToolResponse::ok(
                if merge.mergeable {
                    "merge admitted"
                } else {
                    "merge blocked"
                },
                serde_json::json!({
                    "pr": pr_id.to_string(),
                    "mergeable": merge.mergeable,
                    "blockers": merge.blockers,
                }),
            ),
            Err(err) => ToolResponse::error(err.to_string()),
        }
    }
}

fn str_arg(args: &Value, key: &str) -> String {
    match args.get(key).and_then(Value::as_str) {
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

/// Read an id-like arg that the MCP schema types as an integer but the bridge
/// models as a string (e.g. `repo`).
fn string_or_int(args: &Value, key: &str) -> String {
    match args.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

impl ToolBackend for BridgeBackend {
    fn call(&self, tool: &str, args: Value, ctx: &McpCallContext) -> anyhow::Result<ToolResponse> {
        let resp = match tool {
            "propose_patch" => self.propose_patch(&args, ctx),
            "request_merge" => self.request_merge(&args, ctx),
            // Every other tool (reads, bug_*, etc.) keeps its deterministic
            // delegate behavior.
            _ => self.inner.call(tool, args, ctx)?,
        };
        Ok(resp)
    }

    fn list(&self) -> Vec<ToolDescriptor> {
        ToolBackend::list(&self.inner)
    }
}

impl BugStore for BridgeBackend {
    fn submit(&self, report: Value, idempotency_key: Option<String>) -> anyhow::Result<Value> {
        self.inner.submit(report, idempotency_key)
    }
    fn list(
        &self,
        project: Option<String>,
        status: Option<String>,
        sort: Option<String>,
    ) -> anyhow::Result<Value> {
        BugStore::list(&self.inner, project, status, sort)
    }
    fn show(&self, bug_id: &str) -> anyhow::Result<Value> {
        self.inner.show(bug_id)
    }
    fn ready(&self, project: Option<String>) -> anyhow::Result<Value> {
        self.inner.ready(project)
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
        self.inner
            .update(bug_id, status, severity, priority, component, owner)
    }
    fn record_attempt(&self, bug_id: &str, attempt: Value) -> anyhow::Result<Value> {
        self.inner.record_attempt(bug_id, attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> McpCallContext {
        McpCallContext::mcp("req-1", "tester", crate::MCP_PROTOCOL_VERSION)
    }

    fn propose_args(file_path: &str) -> Value {
        serde_json::json!({
            "repo": 42,
            "branch_name": "agent/fix",
            "base_ref": "main",
            "commit_message": "fix",
            "modifications": [ { "file_path": file_path, "content": "x" } ],
        })
    }

    #[test]
    fn propose_patch_in_scope_performs_scoped_op() {
        let backend =
            BridgeBackend::with_allowed_paths(vec!["crates/jeryu_agentbridge/".to_string()]);
        let resp = backend
            .call(
                "propose_patch",
                propose_args("crates/jeryu_agentbridge/src/api.rs"),
                &ctx(),
            )
            .expect("call ok");
        assert!(resp.success, "{}", resp.message);
        let data = resp.data.expect("data");
        assert!(
            data.get("receipt_id")
                .and_then(Value::as_str)
                .is_some_and(|r| r.starts_with("receipt")),
            "expected a real receipt id, got {data:?}"
        );
        assert_eq!(
            data["changed_paths"],
            serde_json::json!(["crates/jeryu_agentbridge/src/api.rs"])
        );
    }

    #[test]
    fn propose_patch_out_of_scope_is_denied() {
        let backend =
            BridgeBackend::with_allowed_paths(vec!["crates/jeryu_agentbridge/".to_string()]);
        let resp = backend
            .call(
                "propose_patch",
                // Outside the single allowed prefix.
                propose_args("crates/jeryu_proof/src/engine.rs"),
                &ctx(),
            )
            .expect("call ok");
        assert!(!resp.success, "out-of-scope write must be denied");
        assert!(
            resp.message.contains("out-of-scope"),
            "unexpected message: {}",
            resp.message
        );
        assert!(resp.data.is_none());
    }

    #[test]
    fn request_merge_evaluates_through_proof_gate() {
        let backend =
            BridgeBackend::with_allowed_paths(vec!["crates/jeryu_agentbridge/".to_string()]);
        // First propose so the PR exists in the bridge.
        backend
            .call(
                "propose_patch",
                propose_args("crates/jeryu_agentbridge/src/api.rs"),
                &ctx(),
            )
            .expect("propose ok");
        let resp = backend
            .call(
                "request_merge",
                serde_json::json!({
                    "repo": 42,
                    "pr_number": 1,
                    "source_branch": "agent/fix",
                    "target_branch": "main",
                }),
                &ctx(),
            )
            .expect("merge call ok");
        assert!(resp.success, "{}", resp.message);
        let data = resp.data.expect("data");
        // The proof gate requires a witness before queue admission, so a freshly
        // proposed PR is not yet mergeable but the evaluation is real.
        assert_eq!(data["mergeable"], serde_json::json!(false));
        assert!(
            data["blockers"].as_array().is_some_and(|b| !b.is_empty()),
            "expected real blockers from the proof gate"
        );
    }

    #[test]
    fn request_merge_unknown_pr_errors() {
        let backend = BridgeBackend::new();
        let resp = backend
            .call(
                "request_merge",
                serde_json::json!({ "repo": 7, "source_branch": "nope", "target_branch": "main" }),
                &ctx(),
            )
            .expect("call ok");
        assert!(!resp.success);
    }

    #[test]
    fn read_and_bug_tools_delegate() {
        let backend = BridgeBackend::new();
        let resp = backend
            .call("get_system_snapshot", serde_json::json!({}), &ctx())
            .expect("call ok");
        assert!(resp.success);

        let bug = backend
            .call(
                "bug_submit",
                serde_json::json!({ "report": { "title": "t" } }),
                &ctx(),
            )
            .expect("bug submit ok");
        assert!(bug.success);
    }
}
