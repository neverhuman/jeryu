//! Web-mounted MCP backend over the same live repo read model as REST.

use std::sync::Arc;

use jeryu_mcp::{McpCallContext, MemoryBackend, ToolBackend, ToolDescriptor, ToolResponse};
use serde_json::Value;

use super::WebState;
use super::code::{self, CodeReadQuery, CodeSearchQuery};
use super::ecosystem;
use super::repositories::{find_repo, repo_list_response};

pub(super) struct WebMcpBackend {
    state: Arc<WebState>,
    inner: MemoryBackend,
}

impl WebMcpBackend {
    pub(super) fn new(state: Arc<WebState>) -> Self {
        Self {
            state,
            inner: MemoryBackend::new(),
        }
    }
}

impl ToolBackend for WebMcpBackend {
    fn call(&self, tool: &str, args: Value, ctx: &McpCallContext) -> anyhow::Result<ToolResponse> {
        let response = match tool {
            "repo_list" => {
                ToolResponse::ok("repos", serde_json::json!(repo_list_response(&self.state)))
            }
            "repo_tree" => {
                let (repo, query) = read_query(&self.state, args)?;
                match code::tree_response(&self.state, &repo, query) {
                    Ok(tree) => ToolResponse::ok("repo tree", serde_json::json!(tree)),
                    Err(err) => ToolResponse::error(err.to_string()),
                }
            }
            "repo_blob" => {
                let (repo, query) = read_query(&self.state, args)?;
                match code::blob_response(&self.state, &repo, query) {
                    Ok(blob) => ToolResponse::ok("repo blob", serde_json::json!(blob)),
                    Err(err) => ToolResponse::error(err.to_string()),
                }
            }
            "repo_search" => {
                let repo_id = repo_arg(&args)?;
                let repo = find_repo(&self.state, &repo_id)
                    .ok_or_else(|| anyhow::anyhow!("repository not found: {repo_id}"))?;
                let query = CodeSearchQuery {
                    ref_name: optional_string(&args, "ref")
                        .or_else(|| optional_string(&args, "refName")),
                    path: optional_string(&args, "path"),
                    q: required_string(&args, "q").or_else(|_| required_string(&args, "query"))?,
                    limit: args
                        .get("limit")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize),
                };
                match code::search_response(&self.state, &repo, query) {
                    Ok(results) => ToolResponse::ok("repo search", serde_json::json!(results)),
                    Err(err) => ToolResponse::error(err.to_string()),
                }
            }
            "ecosystem_graph" => ToolResponse::ok(
                "ecosystem graph",
                serde_json::json!(ecosystem::ecosystem_response(&self.state)),
            ),
            _ => self.inner.call(tool, args, ctx)?,
        };
        Ok(response)
    }

    fn list(&self) -> Vec<ToolDescriptor> {
        self.inner.list()
    }
}

fn read_query(
    state: &WebState,
    args: Value,
) -> anyhow::Result<(jeryu_core::Repository, CodeReadQuery)> {
    let repo_id = repo_arg(&args)?;
    let repo = find_repo(state, &repo_id)
        .ok_or_else(|| anyhow::anyhow!("repository not found: {repo_id}"))?;
    let query = CodeReadQuery {
        ref_name: optional_string(&args, "ref").or_else(|| optional_string(&args, "refName")),
        path: optional_string(&args, "path"),
    };
    Ok((repo, query))
}

fn repo_arg(args: &Value) -> anyhow::Result<String> {
    required_string(args, "repo_id")
        .or_else(|_| required_string(args, "repoId"))
        .or_else(|_| required_string(args, "repo"))
}

fn required_string(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
