//! Agent-edit command adapter.

use std::io::{Read, Write};
use std::net::TcpStream;

use crate::cli::{
    AgentAuthCommands, AgentCommands, AgentControlArgs, AgentExportPrArgs, AgentRunArgs,
    AgentToolArg,
};
use crate::client::{
    AgentControl, AgentExportPrRequest, AgentRunRequest, AgentTool, ClientError, ClientResult,
    ForgeClient,
};
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
                let value = http_json(
                    api_url,
                    "GET",
                    &format!("/api/v1/agent-runs/{run_id}"),
                    None,
                )?;
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
        let value = http_json(api_url, "POST", "/api/v1/agent-runs", Some(body))?;
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
        let value = http_json(
            api_url,
            "POST",
            &format!("/api/v1/agent-runs/{}/control", args.run_id),
            Some(json!({ "command": command })),
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
    let value = http_json(
        api_url,
        "GET",
        &format!("/api/v1/agent-runs/{run_id}/events?after_seq={after_seq}&limit={limit}"),
        None,
    )?;
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
        let value = http_json(
            api_url,
            "POST",
            &format!("/api/v1/agent-runs/{}/export_pr", args.run_id),
            Some(body),
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

fn http_json(api_url: &str, method: &str, path: &str, body: Option<Value>) -> ClientResult<Value> {
    let endpoint = Endpoint::parse(api_url)?;
    let body_text = body.map(|value| value.to_string()).unwrap_or_default();
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|err| ClientError::NotWired(format!("connect {}: {err}", endpoint.origin())))?;
    let full_path = format!("{}{}", endpoint.base_path, path);
    let request = if body_text.is_empty() {
        format!(
            "{method} {full_path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            endpoint.host_header()
        )
    } else {
        format!(
            "{method} {full_path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            endpoint.host_header(),
            body_text.len(),
            body_text
        )
    };
    stream
        .write_all(request.as_bytes())
        .map_err(|err| ClientError::NotWired(format!("write request: {err}")))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| ClientError::NotWired(format!("read response: {err}")))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| ClientError::Invalid("malformed HTTP response".to_string()))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let value = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body)
            .map_err(|err| ClientError::Invalid(format!("parse JSON response: {err}")))?
    };
    if !(200..300).contains(&status) {
        return Err(ClientError::Conflict(format!(
            "HTTP {status}: {}",
            value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or(body.trim())
        )));
    }
    Ok(value)
}

struct Endpoint {
    host: String,
    port: u16,
    base_path: String,
}

impl Endpoint {
    fn parse(raw: &str) -> ClientResult<Self> {
        let rest = raw.strip_prefix("http://").ok_or_else(|| {
            ClientError::Invalid("only http:// API URLs are supported".to_string())
        })?;
        let (authority, base_path) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = authority
            .rsplit_once(':')
            .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
            .unwrap_or((authority, 80));
        if host.is_empty() {
            return Err(ClientError::Invalid("API URL host is empty".to_string()));
        }
        Ok(Self {
            host: host.to_string(),
            port,
            base_path: if base_path.is_empty() {
                String::new()
            } else {
                format!("/{base_path}")
            },
        })
    }

    fn host_header(&self) -> String {
        if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.host_header())
    }
}

fn map_tool(value: AgentToolArg) -> AgentTool {
    match value {
        AgentToolArg::Codex => AgentTool::Codex,
        AgentToolArg::Claude => AgentTool::Claude,
        AgentToolArg::Jekko => AgentTool::Jekko,
    }
}
