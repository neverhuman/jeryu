use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

// McpCore and McpSessionState are declared in core.rs (our parent module).
// Because this file is included via #[path] in core.rs, super = the mcp module,
// and the types are available via `super::core::*` which is `crate::mcp::core::*`.
use crate::mcp::core::{McpCore, McpSessionState};

pub async fn start_mcp_stdio(client: crate::gitlab_client::GitlabClient) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = BufWriter::new(tokio::io::stdout());
    let mut lines = stdin.lines();
    let core = McpCore::new(client);
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
