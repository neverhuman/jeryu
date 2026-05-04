use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::install::expand_tilde;

const DEFAULT_REMOTE_PREFIX: &str = "~/.jeryu";
const DEFAULT_REMOTE_BIN: &str = "~/.jeryu/bin/jeryu";
const DEFAULT_HTTP_PORT: u16 = 8929;
const DEFAULT_SSH_PORT: u16 = 2224;
const DEFAULT_VAULT_PORT: u16 = 18200;
const DEFAULT_WEBHOOK_PORT: u16 = 9777;
const DEFAULT_SSH_PORT_NUMBER: u16 = 22;

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
}

#[derive(Debug, Clone)]
pub struct RemoteCommonOptions {
    pub dry_run: bool,
    pub json: bool,
    pub yes: bool,
}

#[derive(Debug, Clone)]
pub enum RemoteAction {
    Install {
        target: String,
        alias: Option<String>,
        setup_key: bool,
        identity: Option<PathBuf>,
    },
    Update {
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
            };
            remote_install(cfg, setup_key, &opts).await
        }
        RemoteAction::Update { alias } => {
            let cfg = load_remote_config(&alias)?;
            remote_update(&cfg, &opts).await
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

async fn remote_install(
    cfg: RemoteConfig,
    setup_key: bool,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "remote-install",
                "config": &cfg,
                "setup_key": setup_key,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("Remote install for {}", cfg.alias);
    }
    if opts.dry_run {
        return Ok(0);
    }

    ensure_remote_key(&cfg, setup_key).await?;
    upload_current_binary(&cfg).await?;
    remote_bootstrap(&cfg).await?;
    ensure_remote_service(&cfg).await?;
    save_remote_config(&cfg)?;
    println!("remote host ready: {} ({})", cfg.alias, cfg.target);
    Ok(0)
}

async fn remote_update(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
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
    ensure_remote_service(cfg).await?;
    remote_service(cfg, "restart", opts).await
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
    let cmd = "journalctl --user -u jeryu -n 100 --no-pager";
    run_remote_shell(cfg, cmd, opts.dry_run).await?;
    Ok(0)
}

async fn remote_service(
    cfg: &RemoteConfig,
    action: &str,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    let cmd = format!("systemctl --user {action} jeryu.service");
    run_remote_shell(cfg, &cmd, opts.dry_run).await?;
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
    let cmd = "systemctl --user disable --now jeryu.service >/dev/null 2>&1 || true; rm -f \"$HOME/.jeryu/bin/jeryu\" \"$HOME/.config/systemd/user/jeryu.service\"; systemctl --user daemon-reload";
    run_remote_shell(cfg, &cmd, false).await?;
    let _ = fs::remove_file(config_path(&cfg.alias));
    Ok(0)
}

async fn remote_bootstrap(cfg: &RemoteConfig) -> Result<()> {
    let _ = run_remote_binary(cfg, &["init"], false).await?;
    Ok(())
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
    let service_active =
        run_remote_shell_status(cfg, "systemctl --user is-active jeryu.service").await?;
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
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("remote_bin"));
        assert!(text.contains("~/.jeryu/bin/jeryu"));
    }
}
