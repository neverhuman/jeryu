use super::*;

pub(crate) async fn remote_install(
    cfg: RemoteConfig,
    setup_key: bool,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    let plan = build_remote_plan(&cfg, setup_key, opts);
    if plan.options.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_remote_plan(&plan);
    }
    if plan.options.dry_run {
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

pub(crate) async fn remote_refresh(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
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
        println!("Remote refresh for {}", cfg.alias);
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

pub(crate) async fn remote_doctor(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let report = collect_report(cfg).await?;
    print_remote_report("doctor", &report, opts)?;
    if !report.installed {
        bail!("remote binary not installed");
    }
    if !report.docker_ready {
        bail!("remote docker is not ready");
    }
    Ok(0)
}

pub(crate) async fn remote_status(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let report = collect_report(cfg).await?;
    print_remote_report("status", &report, opts)?;
    Ok(0)
}

pub(crate) async fn remote_logs(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    print_action_envelope(
        opts,
        serde_json::json!({
            "alias": cfg.alias,
            "target": cfg.target,
            "action": "logs",
        }),
    )?;
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

pub(crate) async fn remote_service(
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

pub(crate) async fn remote_ssh(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let mut command = Command::new("ssh");
    command.args(ssh_args(cfg));
    command.arg(&cfg.target);
    if opts.dry_run {
        println!("dry-run: ssh {}", cfg.target);
        return Ok(0);
    }
    run_interactive_ssh(command, "ssh", "opening ssh session").await
}

pub(crate) async fn remote_run(
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
    run_interactive_ssh(cmd, "remote command", "running remote command").await
}

pub(crate) async fn remote_tunnel(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
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
    push_local_forward(&mut cmd, cfg.local_http_port, DEFAULT_HTTP_PORT);
    push_local_forward(&mut cmd, cfg.local_ssh_port, DEFAULT_SSH_PORT);
    push_local_forward(&mut cmd, cfg.local_vault_port, DEFAULT_VAULT_PORT);
    push_local_forward(&mut cmd, cfg.local_webhook_port, DEFAULT_WEBHOOK_PORT);
    cmd.arg(&cfg.target);
    run_interactive_ssh(cmd, "ssh tunnel", "opening ssh tunnel").await
}

#[path = "remote_ops_support.rs"]
mod support;
pub(crate) use support::*;
