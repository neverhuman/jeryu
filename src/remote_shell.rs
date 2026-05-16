use super::*;

pub(super) fn ssh_bash_command(cfg: &RemoteConfig, script: &str) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg("bash");
    cmd.arg("-lc");
    cmd.arg(script);
    cmd
}

pub(super) async fn capture_ssh_bash_output(
    cfg: &RemoteConfig,
    script: &str,
    context_msg: &'static str,
) -> Result<std::process::Output> {
    ssh_bash_command(cfg, script)
        .output()
        .await
        .context(context_msg)
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub(crate) fn push_local_forward(cmd: &mut Command, local_port: u16, remote_port: u16) {
    cmd.arg("-L");
    cmd.arg(format!(
        "127.0.0.1:{}:127.0.0.1:{}",
        local_port, remote_port
    ));
}

pub(crate) fn print_action_envelope(
    opts: &RemoteCommonOptions,
    payload: serde_json::Value,
) -> Result<()> {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    Ok(())
}

pub(crate) fn print_remote_report(
    label: &str,
    report: &RemoteReport,
    opts: &RemoteCommonOptions,
) -> Result<()> {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("Remote {}: {}", label, report.alias);
        println!("  target:         {}", report.target);
        println!("  binary:         {}", report.remote_bin);
        println!("  installed:      {}", report.installed);
        println!("  service active: {}", report.service_active);
        println!("  docker ready:   {}", report.docker_ready);
        if label == "doctor"
            && let Some(version) = &report.version_output
        {
            println!("  version:        {}", version.trim());
        }
    }
    Ok(())
}

pub(crate) async fn remote_uninstall(
    cfg: &RemoteConfig,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    print_action_envelope(
        opts,
        serde_json::json!({
            "action": "remote-uninstall",
            "alias": cfg.alias,
            "target": cfg.target,
            "dry_run": opts.dry_run,
        }),
    )?;
    if opts.dry_run {
        return Ok(0);
    }
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = "systemctl --user disable --now jeryu.service >/dev/null 2>&1 || true; rm -f \"$HOME/.jeryu/bin/jeryu\" \"$HOME/.config/systemd/user/jeryu.service\"; systemctl --user daemon-reload";
            run_remote_shell(cfg, cmd, false).await?;
        }
        ServiceMode::Manual => {
            let cmd = "rm -f \"$HOME/.jeryu/bin/jeryu\"";
            run_remote_shell(cfg, cmd, false).await?;
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    let _ = fs::remove_file(config_path(&cfg.alias));
    Ok(0)
}

pub(crate) async fn probe_remote(cfg: &RemoteConfig) -> Result<RemotePreflight> {
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

pub(crate) async fn remote_bootstrap(cfg: &RemoteConfig) -> Result<()> {
    let _ = run_remote_binary(cfg, &["init"], false).await?;
    Ok(())
}

pub(crate) async fn manual_service_active(cfg: &RemoteConfig) -> Result<bool> {
    run_remote_shell_status(cfg, "pgrep -f 'jeryu serve' >/dev/null 2>&1").await
}

pub(crate) async fn ensure_remote_service(cfg: &RemoteConfig) -> Result<()> {
    let unit = r#"[Unit]
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
    .to_string();
    let script = format!(
        "mkdir -p \"$HOME/.config/systemd/user\" \"$HOME/.jeryu/bin\" \"$HOME/.jeryu\" && cat > \"$HOME/.config/systemd/user/jeryu.service\" <<'EOF'\n{}\nEOF\nsystemctl --user daemon-reload\nsystemctl --user enable --now jeryu.service",
        unit
    );
    run_remote_shell(cfg, &script, false).await
}

pub(crate) async fn collect_report(cfg: &RemoteConfig) -> Result<RemoteReport> {
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

pub(crate) fn print_manual_service_guidance(cfg: &RemoteConfig) {
    println!("manual service guidance for {}:", cfg.alias);
    println!("  - keep {} available on the remote host", cfg.remote_bin);
    println!("  - run: {} serve", cfg.remote_bin);
    println!("  - if you want a user unit later, create ~/.config/systemd/user/jeryu.service");
}

pub(crate) async fn ensure_remote_key(cfg: &RemoteConfig, setup_key: bool) -> Result<()> {
    if !setup_key {
        return Ok(());
    }
    let identity = match cfg.identity.as_deref() {
        Some(identity) => PathBuf::from(identity),
        None => expand_tilde(format!("~/.ssh/jeryu_{}_ed25519", cfg.alias)),
    };
    if !identity.exists() {
        if let Some(parent) = identity.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut keygen = Command::new("ssh-keygen");
        keygen.args(["-t", "ed25519", "-f"]);
        keygen.arg(&identity);
        keygen.args(["-N", "", "-C", &format!("jeryu-{}", cfg.alias)]);
        crate::exec::run_status_check(&mut keygen, "ssh-keygen failed").await?;
    }
    let pubkey = fs::read_to_string(identity.with_extension("pub"))
        .with_context(|| format!("reading {}", identity.with_extension("pub").display()))?;
    let script = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && grep -qxF -- {} ~/.ssh/authorized_keys || printf '%s\\n' {} >> ~/.ssh/authorized_keys",
        shell_single_quote(pubkey.trim()),
        shell_single_quote(pubkey.trim())
    );
    run_remote_shell(cfg, &script, false).await
}

pub(crate) async fn upload_current_binary(cfg: &RemoteConfig) -> Result<()> {
    let local = if let Ok(override_path) = std::env::var("JERYU_REMOTE_BINARY_PATH") {
        std::path::PathBuf::from(override_path)
    } else {
        std::env::current_exe().context("locating current executable")?
    };
    let script = r#"mkdir -p "$HOME/.jeryu/bin" && cat > "$HOME/.jeryu/bin/jeryu.tmp" && install -m 0755 "$HOME/.jeryu/bin/jeryu.tmp" "$HOME/.jeryu/bin/jeryu" && rm -f "$HOME/.jeryu/bin/jeryu.tmp""#;
    let started = Instant::now();
    println!("uploading {} to {}...", local.display(), cfg.target);
    let bytes = fs::read(&local).with_context(|| format!("reading {}", local.display()))?;
    let mut cmd = ssh_bash_command(cfg, script);
    crate::exec::run_with_stdin(&mut cmd, &bytes, "ssh upload failed").await?;
    println!(
        "uploaded remote binary in {}s",
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

pub(crate) async fn run_remote_binary(
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

pub(crate) async fn run_interactive_ssh(
    mut cmd: Command,
    _label: &'static str,
    context_msg: &'static str,
) -> Result<i32> {
    crate::exec::run_status_check(&mut cmd, context_msg).await?;
    Ok(0)
}

pub(crate) async fn run_remote_shell(
    cfg: &RemoteConfig,
    script: &str,
    allow_fail: bool,
) -> Result<()> {
    let output = capture_ssh_bash_output(cfg, script, "running remote shell").await?;
    if output.status.success() || allow_fail {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
}

pub(crate) async fn run_remote_shell_status(cfg: &RemoteConfig, script: &str) -> Result<bool> {
    let output = capture_ssh_bash_output(cfg, script, "running remote shell status").await?;
    Ok(output.status.success())
}

pub(crate) async fn run_remote_shell_capture(
    cfg: &RemoteConfig,
    script: &str,
) -> Result<Option<String>> {
    let output = capture_ssh_bash_output(cfg, script, "running remote shell capture").await?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod remote_shell_tests;
