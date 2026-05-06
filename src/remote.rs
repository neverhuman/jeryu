//! Owner: Remote SSH install and day-two management UX
//! Proof: `cargo test -p jeryu -- remote`
//! Invariants: Remote install dry-runs stay side-effect free; network mutations happen only after confirmation.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::install::{ColorMode, InteractiveMode, expand_tilde};

const DEFAULT_REMOTE_PREFIX: &str = "~/.jeryu";
const DEFAULT_REMOTE_BIN: &str = "~/.jeryu/bin/jeryu";
const DEFAULT_HTTP_PORT: u16 = 8929;
const DEFAULT_SSH_PORT: u16 = 2224;
const DEFAULT_VAULT_PORT: u16 = 18200;
const DEFAULT_WEBHOOK_PORT: u16 = 9777;
const DEFAULT_SSH_PORT_NUMBER: u16 = 22;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
pub enum ServiceMode {
    Auto,
    User,
    Manual,
}

impl Default for ServiceMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub alias: String,
    pub target: String,
    pub ssh_port: u16,
    pub identity: Option<String>,
    pub remote_prefix: String,
    pub remote_bin: String,
    pub local_http_port: u16,
    pub local_ssh_port: u16,
    pub local_vault_port: u16,
    pub local_webhook_port: u16,
    pub created_at_utc: String,
    #[serde(default)]
    pub service_mode: ServiceMode,
}

#[derive(Debug, Clone)]
pub struct RemoteCommonOptions {
    pub dry_run: bool,
    pub json: bool,
    pub yes: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub service_mode: ServiceMode,
    pub verbose: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemotePreflight {
    pub local_ssh: bool,
    pub local_ssh_keygen: bool,
    pub remote_os: Option<String>,
    pub remote_arch: Option<String>,
    pub remote_docker_ready: Option<bool>,
    pub remote_systemd_user: Option<bool>,
    pub remote_disk_free_gb: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteStep {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub command: Option<String>,
    pub requires_network: bool,
    pub estimated_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteInstallPlan {
    pub action: String,
    pub alias: String,
    pub target: String,
    pub ssh_port: u16,
    pub identity: Option<String>,
    pub remote_prefix: String,
    pub remote_bin: String,
    pub local_http_port: u16,
    pub local_ssh_port: u16,
    pub local_vault_port: u16,
    pub local_webhook_port: u16,
    pub dry_run: bool,
    pub json: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub service_mode: ServiceMode,
    pub verbose: bool,
    pub setup_key: bool,
    pub preflight: RemotePreflight,
    pub steps: Vec<RemoteStep>,
}

#[derive(Debug, Clone)]
pub enum RemoteAction {
    Install {
        target: String,
        alias: Option<String>,
        setup_key: bool,
        identity: Option<PathBuf>,
    },
    Refresh {
        alias: String,
    },
    Doctor {
        alias: String,
    },
    Status {
        alias: String,
    },
    Logs {
        alias: String,
    },
    Restart {
        alias: String,
    },
    Stop {
        alias: String,
    },
    Start {
        alias: String,
    },
    Ssh {
        alias: String,
    },
    Run {
        alias: String,
        command: Vec<String>,
    },
    Tunnel {
        alias: String,
    },
    Uninstall {
        alias: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteReport {
    pub alias: String,
    pub target: String,
    pub config_path: String,
    pub remote_prefix: String,
    pub remote_bin: String,
    pub installed: bool,
    pub service_active: bool,
    pub docker_ready: bool,
    pub version_output: Option<String>,
}

pub async fn execute_remote(action: RemoteAction, opts: RemoteCommonOptions) -> Result<i32> {
    match action {
        RemoteAction::Install {
            target,
            alias,
            setup_key,
            identity,
        } => {
            let alias = alias.unwrap_or_else(|| default_alias(&target));
            let cfg = RemoteConfig {
                alias: alias.clone(),
                target,
                ssh_port: DEFAULT_SSH_PORT_NUMBER,
                identity: identity.as_ref().map(|path| path.display().to_string()),
                remote_prefix: DEFAULT_REMOTE_PREFIX.into(),
                remote_bin: DEFAULT_REMOTE_BIN.into(),
                local_http_port: DEFAULT_HTTP_PORT,
                local_ssh_port: DEFAULT_SSH_PORT,
                local_vault_port: DEFAULT_VAULT_PORT,
                local_webhook_port: DEFAULT_WEBHOOK_PORT,
                created_at_utc: Utc::now().to_rfc3339(),
                service_mode: ServiceMode::Auto,
            };
            remote_install(cfg, setup_key, &opts).await
        }
        RemoteAction::Refresh { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_refresh(&cfg, &opts).await
        }
        RemoteAction::Doctor { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_doctor(&cfg, &opts).await
        }
        RemoteAction::Status { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_status(&cfg, &opts).await
        }
        RemoteAction::Logs { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_logs(&cfg, &opts).await
        }
        RemoteAction::Restart { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_service(&cfg, "restart", &opts).await
        }
        RemoteAction::Stop { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_service(&cfg, "stop", &opts).await
        }
        RemoteAction::Start { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_service(&cfg, "start", &opts).await
        }
        RemoteAction::Ssh { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_ssh(&cfg, &opts).await
        }
        RemoteAction::Run { alias, command } => {
            let cfg = load_remote_config(&alias)?;
            remote_run(&cfg, command, &opts).await
        }
        RemoteAction::Tunnel { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_tunnel(&cfg, &opts).await
        }
        RemoteAction::Uninstall { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_uninstall(&cfg, &opts).await
        }
    }
}

fn default_alias(target: &str) -> String {
    target
        .rsplit('@')
        .next()
        .unwrap_or(target)
        .replace([':', '/'], "-")
}

fn remote_root() -> PathBuf {
    expand_tilde("~/.jeryu/remotes")
}

fn config_path(alias: &str) -> PathBuf {
    remote_root().join(format!("{alias}.toml"))
}

fn load_remote_config(alias: &str) -> Result<RemoteConfig> {
    let path = config_path(alias);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("loading remote config {}", path.display()))?;
    let cfg = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(cfg)
}

fn save_remote_config(cfg: &RemoteConfig) -> Result<()> {
    let path = config_path(&cfg.alias);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(cfg).context("serializing remote config")?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn ssh_args(cfg: &RemoteConfig) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        cfg.ssh_port.to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ControlMaster=auto".to_string(),
        "-o".to_string(),
        "ControlPersist=10m".to_string(),
        "-o".to_string(),
        "ControlPath=~/.ssh/jeryu-%r@%h:%p".to_string(),
    ];
    if let Some(identity) = &cfg.identity {
        args.push("-i".to_string());
        args.push(identity.clone());
    }
    args
}

fn should_colorize(mode: ColorMode, json: bool) -> bool {
    if json {
        return false;
    }
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none(),
    }
}

fn should_interactive(mode: InteractiveMode) -> bool {
    match mode {
        InteractiveMode::Always => true,
        InteractiveMode::Never => false,
        InteractiveMode::Auto => io::stdin().is_terminal(),
    }
}

fn color_text(enabled: bool, code: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn status_label(enabled: bool, label: &str, code: &str) -> String {
    format!("[{}]", color_text(enabled, code, label))
}

fn build_remote_plan(
    cfg: &RemoteConfig,
    setup_key: bool,
    opts: &RemoteCommonOptions,
) -> RemoteInstallPlan {
    let preflight = RemotePreflight {
        local_ssh: command_exists("ssh"),
        local_ssh_keygen: command_exists("ssh-keygen"),
        remote_os: None,
        remote_arch: None,
        remote_docker_ready: None,
        remote_systemd_user: None,
        remote_disk_free_gb: None,
    };
    let mut steps = vec![
        RemoteStep {
            id: "preflight".into(),
            label: "check local and remote prerequisites".into(),
            detail: "verify ssh, ssh-keygen, remote OS, docker, and systemd user support".into(),
            command: Some("ssh -p <port> host 'uname -s; uname -m; command -v docker; docker info; command -v systemctl; df -Pk $HOME'".into()),
            requires_network: true,
            estimated_seconds: Some(2),
        },
        RemoteStep {
            id: "binary".into(),
            label: "upload the current binary".into(),
            detail: format!("stream {} to {}", current_exe_string(), cfg.remote_bin),
            command: Some(format!("ssh {} cat > {}", cfg.target, cfg.remote_bin)),
            requires_network: true,
            estimated_seconds: Some(3),
        },
        RemoteStep {
            id: "verify".into(),
            label: "verify remote --version".into(),
            detail: "run the uploaded binary to confirm execution".into(),
            command: Some(format!("ssh {} {} --version", cfg.target, cfg.remote_bin)),
            requires_network: true,
            estimated_seconds: Some(1),
        },
    ];
    let service_detail = match opts.service_mode {
        ServiceMode::Auto => {
            "enable a user systemd unit when available, otherwise print manual serve guidance"
                .to_string()
        }
        ServiceMode::User => "force user systemd setup".to_string(),
        ServiceMode::Manual => "skip systemd and print manual serve guidance".to_string(),
    };
    steps.push(RemoteStep {
        id: "service".into(),
        label: "configure the remote service".into(),
        detail: service_detail,
        command: Some(
            "systemctl --user enable --now jeryu.service || print manual instructions".into(),
        ),
        requires_network: true,
        estimated_seconds: Some(2),
    });
    steps.push(RemoteStep {
        id: "save".into(),
        label: "save the remote metadata".into(),
        detail: "write ~/.jeryu/remotes/<alias>.toml after verification".into(),
        command: Some(format!("write {}", config_path(&cfg.alias).display())),
        requires_network: false,
        estimated_seconds: Some(1),
    });
    RemoteInstallPlan {
        action: "remote-install".into(),
        alias: cfg.alias.clone(),
        target: cfg.target.clone(),
        ssh_port: cfg.ssh_port,
        identity: cfg.identity.clone(),
        remote_prefix: cfg.remote_prefix.clone(),
        remote_bin: cfg.remote_bin.clone(),
        local_http_port: cfg.local_http_port,
        local_ssh_port: cfg.local_ssh_port,
        local_vault_port: cfg.local_vault_port,
        local_webhook_port: cfg.local_webhook_port,
        dry_run: opts.dry_run,
        json: opts.json,
        color: opts.color,
        interactive: opts.interactive,
        service_mode: opts.service_mode,
        verbose: opts.verbose,
        setup_key,
        preflight,
        steps,
    }
}

fn effective_service_mode(
    requested: ServiceMode,
    remote_systemd_user: Option<bool>,
) -> ServiceMode {
    match requested {
        ServiceMode::Auto => {
            if remote_systemd_user.unwrap_or(false) {
                ServiceMode::User
            } else {
                ServiceMode::Manual
            }
        }
        mode => mode,
    }
}

async fn resolve_service_mode(cfg: &RemoteConfig) -> Result<ServiceMode> {
    match cfg.service_mode {
        ServiceMode::Auto => {
            let preflight = probe_remote(cfg).await?;
            Ok(effective_service_mode(
                ServiceMode::Auto,
                preflight.remote_systemd_user,
            ))
        }
        mode => Ok(mode),
    }
}

fn render_remote_plan(plan: &RemoteInstallPlan) {
    let color = should_colorize(plan.color, plan.json);
    println!(
        "{} {}",
        status_label(color, "PLAN", "36;1"),
        color_text(color, "1", "Remote install plan")
    );
    println!("  alias: {}", plan.alias);
    println!("  target: {}", plan.target);
    println!("  remote binary: {}", plan.remote_bin);
    println!("  prefix: {}", plan.remote_prefix);
    println!("  service mode: {:?}", plan.service_mode);
    println!("  setup key: {}", plan.setup_key);
    println!(
        "  local ssh: {}",
        if plan.preflight.local_ssh {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  local ssh-keygen: {}",
        if plan.preflight.local_ssh_keygen {
            "yes"
        } else {
            "no"
        }
    );
    for step in &plan.steps {
        let label = if step.requires_network {
            status_label(color, "RUN", "36;1")
        } else {
            status_label(color, "OK", "32;1")
        };
        println!("  {} {} - {}", label, step.label, step.detail);
        if plan.verbose
            && let Some(command) = &step.command
        {
            println!("      {}", command);
        }
    }
}

fn remote_confirmation(plan: &RemoteInstallPlan, opts: &RemoteCommonOptions) -> Result<bool> {
    if opts.yes {
        return Ok(true);
    }
    if !should_interactive(opts.interactive) {
        bail!("refusing to mutate the remote host without --yes in non-interactive mode");
    }
    render_remote_plan(plan);
    print!("Proceed with remote install? [y/N] ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading confirmation")?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn current_exe_string() -> String {
    env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "(unavailable)".into())
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn remote_install(
    cfg: RemoteConfig,
    setup_key: bool,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    let plan = build_remote_plan(&cfg, setup_key, opts);
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_remote_plan(&plan);
    }
    if opts.dry_run {
        return Ok(0);
    }

    if !remote_confirmation(&plan, opts)? {
        bail!("remote install cancelled");
    }

    if !plan.preflight.local_ssh || !plan.preflight.local_ssh_keygen {
        bail!("ssh and ssh-keygen are required for remote install");
    }
    let preflight = probe_remote(&cfg).await?;
    if opts.verbose {
        println!("remote probe: {:?}", preflight);
    }
    ensure_remote_key(&cfg, setup_key).await?;
    upload_current_binary(&cfg).await?;
    run_remote_binary(&cfg, &["--version"], false).await?;
    remote_bootstrap(&cfg).await?;
    let mut cfg = cfg;
    cfg.service_mode = effective_service_mode(opts.service_mode, preflight.remote_systemd_user);
    match cfg.service_mode {
        ServiceMode::User => {
            if !preflight.remote_systemd_user.unwrap_or(false) {
                bail!("remote host does not expose systemd --user");
            }
            ensure_remote_service(&cfg).await?;
        }
        ServiceMode::Manual => {
            print_manual_service_guidance(&cfg);
        }
        ServiceMode::Auto => panic!("effective service mode should not remain Auto"),
    }
    save_remote_config(&cfg)?;
    println!("remote host ready: {} ({})", cfg.alias, cfg.target);
    Ok(0)
}

async fn remote_refresh(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "remote-update",
                "config": cfg,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("Remote update for {}", cfg.alias);
    }
    if opts.dry_run {
        return Ok(0);
    }
    upload_current_binary(cfg).await?;
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            ensure_remote_service(cfg).await?;
            remote_service(cfg, "restart", opts).await
        }
        ServiceMode::Manual => {
            print_manual_service_guidance(cfg);
            Ok(0)
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
}

async fn remote_doctor(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let report = collect_report(cfg).await?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Remote doctor: {}", cfg.alias);
        println!("  target:         {}", report.target);
        println!("  binary:         {}", report.remote_bin);
        println!("  installed:      {}", report.installed);
        println!("  service active: {}", report.service_active);
        println!("  docker ready:   {}", report.docker_ready);
        if let Some(version) = &report.version_output {
            println!("  version:        {}", version.trim());
        }
    }
    if !report.installed {
        bail!("remote binary not installed");
    }
    if !report.docker_ready {
        bail!("remote docker is not ready");
    }
    Ok(0)
}

async fn remote_status(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let report = collect_report(cfg).await?;
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Remote status: {}", cfg.alias);
        println!("  target:         {}", report.target);
        println!("  binary:         {}", report.remote_bin);
        println!("  installed:      {}", report.installed);
        println!("  service active: {}", report.service_active);
        println!("  docker ready:   {}", report.docker_ready);
    }
    Ok(0)
}

async fn remote_logs(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "alias": cfg.alias,
                "target": cfg.target,
                "action": "logs",
            }))?
        );
    }
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = "journalctl --user -u jeryu -n 100 --no-pager";
            run_remote_shell(cfg, cmd, opts.dry_run).await?;
        }
        ServiceMode::Manual => {
            bail!("remote host uses manual service mode; there is no systemd journal to tail");
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    Ok(0)
}

async fn remote_service(
    cfg: &RemoteConfig,
    action: &str,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = format!("systemctl --user {action} jeryu.service");
            run_remote_shell(cfg, &cmd, opts.dry_run).await?;
        }
        ServiceMode::Manual => {
            bail!(
                "remote host uses manual service mode; use '{}' serve over ssh instead",
                cfg.remote_bin
            );
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    Ok(0)
}

async fn remote_ssh(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let mut command = Command::new("ssh");
    command.args(ssh_args(cfg));
    command.arg(&cfg.target);
    if opts.dry_run {
        println!("dry-run: ssh {}", cfg.target);
        return Ok(0);
    }
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    let status = command.status().await.context("opening ssh session")?;
    if !status.success() {
        bail!("ssh exited with {}", status.code().unwrap_or(-1));
    }
    Ok(0)
}

async fn remote_run(
    cfg: &RemoteConfig,
    command: Vec<String>,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    if command.is_empty() {
        bail!("remote run requires a command after --");
    }
    if opts.dry_run {
        println!("dry-run: {} {}", cfg.remote_bin, command.join(" "));
        return Ok(0);
    }
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg(&cfg.remote_bin);
    cmd.args(&command);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let status = cmd.status().await.context("running remote command")?;
    if !status.success() {
        bail!("remote command exited with {}", status.code().unwrap_or(-1));
    }
    Ok(0)
}

async fn remote_tunnel(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    if opts.dry_run {
        println!(
            "dry-run: ssh -N -L 127.0.0.1:{}:127.0.0.1:{} -L 127.0.0.1:{}:127.0.0.1:{} -L 127.0.0.1:{}:127.0.0.1:{} -L 127.0.0.1:{}:127.0.0.1:{} {}",
            cfg.local_http_port,
            DEFAULT_HTTP_PORT,
            cfg.local_ssh_port,
            DEFAULT_SSH_PORT,
            cfg.local_vault_port,
            DEFAULT_VAULT_PORT,
            cfg.local_webhook_port,
            DEFAULT_WEBHOOK_PORT,
            cfg.target
        );
        return Ok(0);
    }
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg("-N");
    cmd.arg("-L");
    cmd.arg(format!(
        "127.0.0.1:{}:127.0.0.1:{}",
        cfg.local_http_port, DEFAULT_HTTP_PORT
    ));
    cmd.arg("-L");
    cmd.arg(format!(
        "127.0.0.1:{}:127.0.0.1:{}",
        cfg.local_ssh_port, DEFAULT_SSH_PORT
    ));
    cmd.arg("-L");
    cmd.arg(format!(
        "127.0.0.1:{}:127.0.0.1:{}",
        cfg.local_vault_port, DEFAULT_VAULT_PORT
    ));
    cmd.arg("-L");
    cmd.arg(format!(
        "127.0.0.1:{}:127.0.0.1:{}",
        cfg.local_webhook_port, DEFAULT_WEBHOOK_PORT
    ));
    cmd.arg(&cfg.target);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let status = cmd.status().await.context("opening ssh tunnel")?;
    if !status.success() {
        bail!("ssh tunnel exited with {}", status.code().unwrap_or(-1));
    }
    Ok(0)
}

async fn remote_uninstall(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "remote-uninstall",
                "alias": cfg.alias,
                "target": cfg.target,
                "dry_run": opts.dry_run,
            }))?
        );
    }
    if opts.dry_run {
        return Ok(0);
    }
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = "systemctl --user disable --now jeryu.service >/dev/null 2>&1 || true; rm -f \"$HOME/.jeryu/bin/jeryu\" \"$HOME/.config/systemd/user/jeryu.service\"; systemctl --user daemon-reload";
            run_remote_shell(cfg, &cmd, false).await?;
        }
        ServiceMode::Manual => {
            let cmd = "rm -f \"$HOME/.jeryu/bin/jeryu\"";
            run_remote_shell(cfg, &cmd, false).await?;
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    let _ = fs::remove_file(config_path(&cfg.alias));
    Ok(0)
}

async fn probe_remote(cfg: &RemoteConfig) -> Result<RemotePreflight> {
    let remote_os = run_remote_shell_capture(cfg, "uname -s").await?;
    let remote_arch = run_remote_shell_capture(cfg, "uname -m").await?;
    let docker_ready = run_remote_shell_status(cfg, "docker info >/dev/null 2>&1").await?;
    let systemd_user =
        run_remote_shell_status(cfg, "systemctl --user is-system-running >/dev/null 2>&1")
            .await
            .ok();
    let disk_free_gb = run_remote_shell_capture(
        cfg,
        "df -Pk \"$HOME\" | awk 'NR==2 { printf \"%.2f\", $4 / 1024 / 1024 }'",
    )
    .await?
    .and_then(|text| text.trim().parse::<f64>().ok());
    Ok(RemotePreflight {
        local_ssh: command_exists("ssh"),
        local_ssh_keygen: command_exists("ssh-keygen"),
        remote_os,
        remote_arch,
        remote_docker_ready: Some(docker_ready),
        remote_systemd_user: systemd_user,
        remote_disk_free_gb: disk_free_gb,
    })
}

async fn remote_bootstrap(cfg: &RemoteConfig) -> Result<()> {
    let _ = run_remote_binary(cfg, &["init"], false).await?;
    Ok(())
}

async fn manual_service_active(cfg: &RemoteConfig) -> Result<bool> {
    run_remote_shell_status(cfg, "pgrep -f 'jeryu serve' >/dev/null 2>&1").await
}

async fn ensure_remote_service(cfg: &RemoteConfig) -> Result<()> {
    let unit = format!(
        r#"[Unit]
Description=JeRyu remote control plane
After=network-online.target

[Service]
Type=simple
ExecStart=%h/.jeryu/bin/jeryu serve
WorkingDirectory=%h/.jeryu
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
"#
    );
    let script = format!(
        "mkdir -p \"$HOME/.config/systemd/user\" \"$HOME/.jeryu/bin\" \"$HOME/.jeryu\" && cat > \"$HOME/.config/systemd/user/jeryu.service\" <<'EOF'\n{}\nEOF\nsystemctl --user daemon-reload\nsystemctl --user enable --now jeryu.service",
        unit
    );
    run_remote_shell(cfg, &script, false).await
}

async fn collect_report(cfg: &RemoteConfig) -> Result<RemoteReport> {
    let binary_output = run_remote_binary(cfg, &["--version"], true).await?;
    let docker_ready = run_remote_shell_status(cfg, "docker info >/dev/null 2>&1").await?;
    let service_active = match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            run_remote_shell_status(cfg, "systemctl --user is-active jeryu.service").await?
        }
        ServiceMode::Manual => manual_service_active(cfg).await?,
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    };
    Ok(RemoteReport {
        alias: cfg.alias.clone(),
        target: cfg.target.clone(),
        config_path: config_path(&cfg.alias).display().to_string(),
        remote_prefix: cfg.remote_prefix.clone(),
        remote_bin: cfg.remote_bin.clone(),
        installed: binary_output.is_some(),
        service_active,
        docker_ready,
        version_output: binary_output,
    })
}

fn print_manual_service_guidance(cfg: &RemoteConfig) {
    println!("manual service guidance for {}:", cfg.alias);
    println!("  - keep {} available on the remote host", cfg.remote_bin);
    println!("  - run: {} serve", cfg.remote_bin);
    println!("  - if you want a user unit later, create ~/.config/systemd/user/jeryu.service");
}

async fn ensure_remote_key(cfg: &RemoteConfig, setup_key: bool) -> Result<()> {
    if !setup_key {
        return Ok(());
    }
    let identity = cfg
        .identity
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| expand_tilde(format!("~/.ssh/jeryu_{}_ed25519", cfg.alias)));
    if !identity.exists() {
        if let Some(parent) = identity.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut keygen = Command::new("ssh-keygen");
        keygen.args(["-t", "ed25519", "-f"]);
        keygen.arg(&identity);
        keygen.args(["-N", "", "-C", &format!("jeryu-{}", cfg.alias)]);
        let status = keygen.status().await.context("running ssh-keygen")?;
        if !status.success() {
            bail!("ssh-keygen failed");
        }
    }
    let pubkey = fs::read_to_string(identity.with_extension("pub"))
        .with_context(|| format!("reading {}", identity.with_extension("pub").display()))?;
    let script = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && grep -qxF -- {} ~/.ssh/authorized_keys || printf '%s\\n' {} >> ~/.ssh/authorized_keys",
        shell_single_quote(&pubkey.trim()),
        shell_single_quote(&pubkey.trim())
    );
    run_remote_shell(cfg, &script, false).await
}

async fn upload_current_binary(cfg: &RemoteConfig) -> Result<()> {
    let local = std::env::current_exe().context("locating current executable")?;
    let script = r#"mkdir -p "$HOME/.jeryu/bin" && cat > "$HOME/.jeryu/bin/jeryu.tmp" && install -m 0755 "$HOME/.jeryu/bin/jeryu.tmp" "$HOME/.jeryu/bin/jeryu" && rm -f "$HOME/.jeryu/bin/jeryu.tmp""#;
    let started = Instant::now();
    println!("uploading {} to {}...", local.display(), cfg.target);
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg("bash");
    cmd.arg("-lc");
    cmd.arg(script);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let mut child = cmd.spawn().context("starting remote upload")?;
    let mut stdin = child.stdin.take().context("opening ssh stdin")?;
    let bytes = fs::read(&local).with_context(|| format!("reading {}", local.display()))?;
    stdin
        .write_all(&bytes)
        .await
        .context("streaming binary to remote")?;
    drop(stdin);
    let status = child.wait().await.context("finishing remote upload")?;
    if !status.success() {
        bail!("ssh upload exited with {}", status.code().unwrap_or(-1));
    }
    println!(
        "uploaded remote binary in {}s",
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

async fn run_remote_binary(
    cfg: &RemoteConfig,
    args: &[&str],
    allow_fail: bool,
) -> Result<Option<String>> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg(&cfg.remote_bin);
    cmd.args(args);
    let output = cmd.output().await.context("running remote binary")?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else if allow_fail {
        Ok(None)
    } else {
        bail!(
            "remote binary exited with {}",
            output.status.code().unwrap_or(-1)
        );
    }
}

async fn run_remote_shell(cfg: &RemoteConfig, script: &str, allow_fail: bool) -> Result<()> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg("bash");
    cmd.arg("-lc");
    cmd.arg(script);
    let output = cmd.output().await.context("running remote shell")?;
    if output.status.success() || allow_fail {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
}

async fn run_remote_shell_status(cfg: &RemoteConfig, script: &str) -> Result<bool> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg("bash");
    cmd.arg("-lc");
    cmd.arg(script);
    let output = cmd.output().await.context("running remote shell status")?;
    Ok(output.status.success())
}

async fn run_remote_shell_capture(cfg: &RemoteConfig, script: &str) -> Result<Option<String>> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg("bash");
    cmd.arg("-lc");
    cmd.arg(script);
    let output = cmd.output().await.context("running remote shell capture")?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else {
        Ok(None)
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_alias_is_target_tail() {
        assert_eq!(default_alias("deploy@10.0.0.20"), "10.0.0.20");
        assert_eq!(default_alias("xbabe1"), "xbabe1");
    }

    #[test]
    fn config_round_trip_contains_expected_paths() {
        let cfg = RemoteConfig {
            alias: "xbabe1".into(),
            target: "xbabe1".into(),
            ssh_port: 22,
            identity: None,
            remote_prefix: "~/.jeryu".into(),
            remote_bin: "~/.jeryu/bin/jeryu".into(),
            local_http_port: DEFAULT_HTTP_PORT,
            local_ssh_port: DEFAULT_SSH_PORT,
            local_vault_port: DEFAULT_VAULT_PORT,
            local_webhook_port: DEFAULT_WEBHOOK_PORT,
            created_at_utc: "2026-05-04T00:00:00Z".into(),
            service_mode: ServiceMode::Auto,
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("remote_bin"));
        assert!(text.contains("~/.jeryu/bin/jeryu"));
        assert!(text.contains("service_mode"));
    }

    #[test]
    fn remote_install_plan_includes_service_mode_and_steps() {
        let cfg = RemoteConfig {
            alias: "xbabe1".into(),
            target: "xbabe1".into(),
            ssh_port: 22,
            identity: None,
            remote_prefix: "~/.jeryu".into(),
            remote_bin: "~/.jeryu/bin/jeryu".into(),
            local_http_port: DEFAULT_HTTP_PORT,
            local_ssh_port: DEFAULT_SSH_PORT,
            local_vault_port: DEFAULT_VAULT_PORT,
            local_webhook_port: DEFAULT_WEBHOOK_PORT,
            created_at_utc: "2026-05-04T00:00:00Z".into(),
            service_mode: ServiceMode::Auto,
        };
        let plan = build_remote_plan(
            &cfg,
            true,
            &RemoteCommonOptions {
                dry_run: true,
                json: true,
                yes: true,
                color: ColorMode::Never,
                interactive: InteractiveMode::Never,
                service_mode: ServiceMode::Manual,
                verbose: false,
            },
        );
        let rendered = serde_json::to_value(&plan).unwrap();
        assert_eq!(rendered["service_mode"], "Manual");
        assert_eq!(rendered["setup_key"], true);
        assert!(
            rendered["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|step| { step["id"].as_str().unwrap() == "verify" })
        );
    }

    #[test]
    fn remote_plan_is_json_serializable_without_network() {
        let cfg = RemoteConfig {
            alias: "xbabe1".into(),
            target: "xbabe1".into(),
            ssh_port: 22,
            identity: None,
            remote_prefix: "~/.jeryu".into(),
            remote_bin: "~/.jeryu/bin/jeryu".into(),
            local_http_port: DEFAULT_HTTP_PORT,
            local_ssh_port: DEFAULT_SSH_PORT,
            local_vault_port: DEFAULT_VAULT_PORT,
            local_webhook_port: DEFAULT_WEBHOOK_PORT,
            created_at_utc: "2026-05-04T00:00:00Z".into(),
            service_mode: ServiceMode::Auto,
        };
        let plan = build_remote_plan(
            &cfg,
            false,
            &RemoteCommonOptions {
                dry_run: true,
                json: false,
                yes: true,
                color: ColorMode::Auto,
                interactive: InteractiveMode::Auto,
                service_mode: ServiceMode::Auto,
                verbose: false,
            },
        );
        assert_eq!(plan.action, "remote-install");
        assert!(!plan.preflight.local_ssh_keygen || plan.preflight.local_ssh);
    }

    #[test]
    fn effective_service_mode_resolves_auto_by_preflight() {
        assert_eq!(
            effective_service_mode(ServiceMode::Auto, Some(true)),
            ServiceMode::User
        );
        assert_eq!(
            effective_service_mode(ServiceMode::Auto, Some(false)),
            ServiceMode::Manual
        );
        assert_eq!(
            effective_service_mode(ServiceMode::User, Some(false)),
            ServiceMode::User
        );
        assert_eq!(
            effective_service_mode(ServiceMode::Manual, Some(true)),
            ServiceMode::Manual
        );
    }

    #[test]
    fn remote_config_defaults_service_mode_when_missing() {
        let text = r#"
alias = "xbabe1"
target = "xbabe1"
ssh_port = 22
remote_prefix = "~/.jeryu"
remote_bin = "~/.jeryu/bin/jeryu"
local_http_port = 8929
local_ssh_port = 2224
local_vault_port = 18200
local_webhook_port = 9777
created_at_utc = "2026-05-04T00:00:00Z"
"#;
        let cfg: RemoteConfig = toml::from_str(text).unwrap();
        assert_eq!(cfg.service_mode, ServiceMode::Auto);
    }
}
