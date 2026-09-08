use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use jeryu_agentbridge::cli_registry::{AgentCli, ModelSelect, adapter_for};

const PROMPT: &str = "Reply with exactly the single word: READY";

fn requested_clis() -> Vec<AgentCli> {
    let Ok(raw) = std::env::var("JERYU_AGENT_LIVE_SMOKE") else {
        return Vec::new();
    };
    raw.split(',')
        .filter_map(|token| token.trim().parse::<AgentCli>().ok())
        .collect()
}

fn program_for(cli: AgentCli) -> String {
    let key = format!("JERYU_AGENT_{}_BIN", cli.as_str().to_ascii_uppercase());
    std::env::var(key).unwrap_or_else(|_| cli.default_program().to_string())
}

fn smoke_model(cli: AgentCli) -> ModelSelect {
    let key = format!("JERYU_AGENT_{}_MODEL", cli.as_str().to_ascii_uppercase());
    let default = match cli {
        AgentCli::Claude => "claude-haiku-4-5-20251001",
        AgentCli::Codex => "gpt-5.4-mini",
        AgentCli::Jekko => "claude-haiku-4-5-20251001",
    };
    let mut model = ModelSelect::new(std::env::var(key).unwrap_or_else(|_| default.to_string()));
    if cli == AgentCli::Jekko {
        model = model.with_provider(
            std::env::var("JERYU_AGENT_JEKKO_PROVIDER").unwrap_or_else(|_| "anthropic".to_string()),
        );
    }
    model
}

fn program_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "live: hits real model APIs and consumes quota"]
fn live_clis_respond_authenticated() {
    let clis = requested_clis();
    if clis.is_empty() {
        eprintln!("SKIP live smoke: set JERYU_AGENT_LIVE_SMOKE=claude,codex");
        return;
    }

    for cli in clis {
        let program = program_for(cli);
        if !program_available(&program) {
            eprintln!("SKIP {cli}: program '{program}' not runnable on this host");
            continue;
        }

        let model = smoke_model(cli);
        let mut plan = adapter_for(cli).build_launch(&program, &model, false);
        assert!(plan.prompt_on_stdin);

        if cli == AgentCli::Codex {
            for arg in &mut plan.args {
                if arg == "workspace-write" {
                    *arg = "read-only".to_string();
                }
            }
            plan.args.push("--skip-git-repo-check".to_string());
        }

        let workdir = tempfile::tempdir().expect("temp workdir");
        let started = Instant::now();
        let mut child = Command::new(&plan.program)
            .args(&plan.args)
            .envs(&plan.extra_env)
            .current_dir(workdir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {cli} ({}): {error}", plan.program));

        child
            .stdin
            .take()
            .expect("child stdin")
            .write_all(PROMPT.as_bytes())
            .expect("write prompt");

        let output = child.wait_with_output().expect("await child");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let elapsed = started.elapsed();

        assert!(
            output.status.success() && stdout.to_uppercase().contains("READY"),
            "{cli} live smoke failed (status={:?}, {:?})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
            output.status.code(),
            elapsed,
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}
