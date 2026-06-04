//! Agent-edit command adapter.

use std::io::Write;

use crate::cli::{
    AgentAuthCommands, AgentCommands, AgentControlArgs, AgentExportPrArgs, AgentRunArgs,
    AgentToolArg,
};
use crate::client::{
    AgentControl, AgentExportPrRequest, AgentRunRequest, AgentTool, ClientError, ClientResult,
    ForgeClient,
};
use crate::commands::render;

/// Run an agent command.
pub fn run(
    client: &dyn ForgeClient,
    json: bool,
    command: AgentCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        AgentCommands::Auth(auth) => run_auth(client, json, auth, out),
        AgentCommands::Run(args) => run_agent(client, json, args, out),
        AgentCommands::Status { run_id } => {
            let status = client.agent_status(&run_id)?;
            let human = format!("agent run {} is {}", status.agent_run_id, status.state);
            render(out, json, &status, &human)
        }
        AgentCommands::Control(args) => run_control(client, json, args, out),
        AgentCommands::ExportPr(args) => run_export_pr(client, json, args, out),
    }
}

fn run_auth(
    client: &dyn ForgeClient,
    json: bool,
    command: AgentAuthCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        AgentAuthCommands::Import { from_host } => {
            let tool = map_tool(from_host);
            let receipt = client.agent_auth_import(tool)?;
            let human = format!(
                "imported {} auth into {}",
                receipt.tool.as_str(),
                receipt.auth_dir
            );
            render(out, json, &receipt, &human)
        }
        AgentAuthCommands::Doctor { tool } => {
            let report = client.agent_auth_doctor(map_tool(tool))?;
            let human = format!("{} auth ok={}", report.tool.as_str(), report.ok);
            render(out, json, &report, &human)
        }
    }
}

fn run_agent(
    client: &dyn ForgeClient,
    json: bool,
    args: AgentRunArgs,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let prompt = std::fs::read_to_string(&args.task_file).map_err(|err| {
        ClientError::Invalid(format!(
            "read task file {}: {err}",
            args.task_file.display()
        ))
    })?;
    let status = client.agent_run(AgentRunRequest {
        repo: args.repo,
        agent: map_tool(args.agent),
        prompt,
        model: args.model,
        effort: args.effort,
        base_ref: args.base_ref,
    })?;
    let human = format!("started agent run {}", status.agent_run_id);
    render(out, json, &status, &human)
}

fn run_control(
    client: &dyn ForgeClient,
    json: bool,
    args: AgentControlArgs,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let mut selected = Vec::new();
    if let Some(text) = args.stdin_text {
        selected.push(AgentControl::StdinText { text });
    }
    if args.interrupt {
        selected.push(AgentControl::Interrupt);
    }
    if args.terminate {
        selected.push(AgentControl::Terminate);
    }
    if selected.len() != 1 {
        return Err(ClientError::Invalid(
            "choose exactly one of --stdin, --interrupt, or --terminate".to_string(),
        ));
    }
    let status = client.agent_control(&args.run_id, selected.remove(0))?;
    let human = format!("agent run {} is {}", status.agent_run_id, status.state);
    render(out, json, &status, &human)
}

fn run_export_pr(
    client: &dyn ForgeClient,
    json: bool,
    args: AgentExportPrArgs,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let exported = client.agent_export_pr(AgentExportPrRequest {
        agent_run_id: args.run_id,
        title: args.title,
        body: args.body,
    })?;
    let human = format!(
        "exported agent run {} to {}",
        exported.agent_run_id, exported.url
    );
    render(out, json, &exported, &human)
}

fn map_tool(value: AgentToolArg) -> AgentTool {
    match value {
        AgentToolArg::Codex => AgentTool::Codex,
        AgentToolArg::Claude => AgentTool::Claude,
        AgentToolArg::Jekko => AgentTool::Jekko,
    }
}
