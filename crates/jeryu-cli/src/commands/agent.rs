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
use crate::commands::api::ApiClient;
use crate::commands::render;
use serde_json::{Value, json};

/// Run an agent command.
pub fn run(
    client: &dyn ForgeClient,
    json: bool,
    api_url: Option<&str>,
    command: AgentCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match command {
        AgentCommands::Auth(auth) => run_auth(client, json, auth, out),
        AgentCommands::Run(args) => run_agent(client, json, api_url, args, out),
        AgentCommands::Status { run_id } => {
            if let Some(api_url) = api_url {
                let value =
                    ApiClient::new(api_url)?.get(&format!("/api/v1/agent-runs/{run_id}"))?;
                return render(
                    out,
                    json,
                    &value,
                    &format!("agent run {run_id} status fetched"),
                );
            }
            let status = client.agent_status(&run_id)?;
            let human = format!("agent run {} is {}", status.agent_run_id, status.state);
            render(out, json, &status, &human)
        }
        AgentCommands::Control(args) => run_control(client, json, api_url, args, out),
        AgentCommands::Follow {
            run_id,
            after_seq,
            limit,
        } => run_follow(json, api_url, &run_id, after_seq, limit, out),
        AgentCommands::ExportPr(args) => run_export_pr(client, json, api_url, args, out),
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
    api_url: Option<&str>,
    args: AgentRunArgs,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let prompt = std::fs::read_to_string(&args.task_file).map_err(|err| {
        ClientError::Invalid(format!(
            "read task file {}: {err}",
            args.task_file.display()
        ))
    })?;
    if let Some(api_url) = api_url {
        let workcell_id = args.workcell_id.ok_or_else(|| {
            ClientError::Invalid("--workcell-id is required for live agent run".to_string())
        })?;
        let runner_epoch = args.runner_epoch.ok_or_else(|| {
            ClientError::Invalid("--runner-epoch is required for live agent run".to_string())
        })?;
        let program = args.program.ok_or_else(|| {
            ClientError::Invalid("--program is required for live agent run".to_string())
        })?;
        let mut body = json!({
            "source": {
                "kind": "workcell",
                "workcell_id": workcell_id,
                "runner_epoch": runner_epoch
            },
            "io_mode": args.io_mode,
            "program": program,
            "args": args.args,
            "prompt": prompt
        });
        if let Some(repo_root) = args.repo_root {
            body["repo_root"] = Value::String(repo_root.to_string_lossy().to_string());
        }
        let value = ApiClient::new(api_url)?.post("/api/v1/agent-runs", body)?;
        let run_id = value
            .get("agent_run_id")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return render(out, json, &value, &format!("started agent run {run_id}"));
    }

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
    api_url: Option<&str>,
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
    if let Some(api_url) = api_url {
        let command = match selected.remove(0) {
            AgentControl::StdinText { text } => json!({"kind": "send_input", "text": text}),
            AgentControl::Interrupt => json!({"kind": "interrupt"}),
            AgentControl::Terminate => json!({"kind": "terminate"}),
        };
        let value = ApiClient::new(api_url)?.post(
            &format!("/api/v1/agent-runs/{}/control", args.run_id),
            json!({ "command": command }),
        )?;
        return render(
            out,
            json,
            &value,
            &format!("sent control to agent run {}", args.run_id),
        );
    }
    let status = client.agent_control(&args.run_id, selected.remove(0))?;
    let human = format!("agent run {} is {}", status.agent_run_id, status.state);
    render(out, json, &status, &human)
}

fn run_follow(
    json_output: bool,
    api_url: Option<&str>,
    run_id: &str,
    after_seq: u64,
    limit: u64,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let Some(api_url) = api_url else {
        return Err(ClientError::NotWired(
            "agent follow requires --api-url or JERYU_API_URL".to_string(),
        ));
    };
    let value = ApiClient::new(api_url)?.get(&format!(
        "/api/v1/agent-runs/{run_id}/events?after_seq={after_seq}&limit={limit}"
    ))?;
    render(
        out,
        json_output,
        &value,
        &format!("fetched events for {run_id}"),
    )
}

fn run_export_pr(
    client: &dyn ForgeClient,
    json: bool,
    api_url: Option<&str>,
    args: AgentExportPrArgs,
    out: &mut dyn Write,
) -> ClientResult<()> {
    if let Some(api_url) = api_url {
        let owner = args.owner.ok_or_else(|| {
            ClientError::Invalid("--owner is required for live export-pr".to_string())
        })?;
        let repo = args.repo.ok_or_else(|| {
            ClientError::Invalid("--repo is required for live export-pr".to_string())
        })?;
        let author = args.author.ok_or_else(|| {
            ClientError::Invalid("--author is required for live export-pr".to_string())
        })?;
        let mut body = json!({
            "owner": owner,
            "repo": repo,
            "author": author,
            "title": args.title,
            "body": args.body,
        });
        if let Some(target_branch) = args.target_branch {
            body["target_branch"] = Value::String(target_branch);
        }
        let value = ApiClient::new(api_url)?.post(
            &format!("/api/v1/agent-runs/{}/export_pr", args.run_id),
            body,
        )?;
        return render(
            out,
            json,
            &value,
            &format!("exported agent run {}", args.run_id),
        );
    }
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
