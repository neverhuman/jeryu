use std::sync::Arc;

use serde_json::Value;

use super::{WebState, codegraph};

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
        if tool != "codegraph.query" {
            return self.inner.call(tool, args, ctx);
        }
        let repo = args
            .get("repo")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("codegraph.query requires repo"))?;
        let query = codegraph_query_from_mcp_args(&args)?;
        match codegraph::query_pack_for_repo(&self.state, repo, query) {
            Ok(pack) => Ok(jeryu_mcp::ToolResponse::ok(
                "codegraph impact pack",
                serde_json::to_value(pack)?,
            )),
            Err(error) => Ok(jeryu_mcp::ToolResponse::error(error.to_string())),
        }
    }

    fn list(&self) -> Vec<jeryu_mcp::ToolDescriptor> {
        self.inner.list()
    }
}

fn codegraph_query_from_mcp_args(args: &Value) -> anyhow::Result<jeryu_codegraph::CodeGraphQuery> {
    let changed_paths = args
        .get("changed_paths")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("codegraph.query requires changed_paths"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .ok_or_else(|| anyhow::anyhow!("changed_paths entries must be strings"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let max_tokens = args
        .get("max_tokens")
        .and_then(Value::as_u64)
        .map(u32::try_from)
        .transpose()?;
    Ok(jeryu_codegraph::CodeGraphQuery {
        ref_name: args
            .get("ref")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string(),
        changed_paths,
        intent: args
            .get("intent")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        question: args
            .get("question")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        max_tokens,
    })
}
