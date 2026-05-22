use super::*;
use std::fs;

#[path = "remote_shell_exec.rs"]
mod remote_shell_exec;
pub(crate) use remote_shell_exec::{
    run_interactive_ssh, run_remote_binary, run_remote_shell, run_remote_shell_capture,
    run_remote_shell_status,
};

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
    // `jeryu init` tries to bring up the local GitLab stack via
    // `docker compose up`, which pulls gitlab/gitlab-ce (~1.5GB image).
    // On constrained remote hosts (small VPS, ephemeral CI containers)
    // this either times out or fails on disk/network. Treat init as
    // advisory — the install completes the binary upload and config
    // write; the operator can run `jeryu init` later with a fresh
    // `jeryu serve` session, getting full error context.
    if let Err(e) = run_remote_binary(cfg, &["init"], false).await {
        eprintln!(
            "warning: 'jeryu init' on remote did not complete cleanly: {e}\n\
             this is non-fatal — the binary is installed and the remote\n\
             config is written. Run `jeryu init` manually on the remote\n\
             once docker has the bandwidth + disk for gitlab/gitlab-ce."
        );
    }
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

#[cfg(test)]
mod remote_shell_tests;
