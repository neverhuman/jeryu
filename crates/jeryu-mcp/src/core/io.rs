//! stdio JSON-RPC server (port of `core_io.rs`).

use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use super::{McpCore, McpSessionState};
use crate::backend::ToolBackend;

/// Run the line-buffered stdio JSON-RPC loop until stdin closes.
pub async fn start_mcp_stdio(backend: Arc<dyn ToolBackend>) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = BufWriter::new(tokio::io::stdout());
    let mut lines = stdin.lines();
    let core = McpCore::new(backend);
    let mut state = McpSessionState::new();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let responses = core.handle_line(&mut state, line).await;
        if responses.is_empty() {
            continue;
        }

        let payload: Vec<u8> = if responses.len() == 1 {
            serde_json::to_vec(&responses[0])?
        } else {
            serde_json::to_vec(&responses)?
        };
        stdout.write_all(&payload).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}
