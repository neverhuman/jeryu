use super::*;

pub(crate) fn default_alias(target: &str) -> String {
    target
        .rsplit('@')
        .next()
        .unwrap_or(target)
        .replace([':', '/'], "-")
}

pub(crate) fn remote_root() -> PathBuf {
    expand_tilde("~/.jeryu/remotes")
}

pub(crate) fn config_path(alias: &str) -> PathBuf {
    remote_root().join(format!("{alias}.toml"))
}

pub(crate) fn load_remote_config(alias: &str) -> Result<RemoteConfig> {
    let path = config_path(alias);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("loading remote config {}", path.display()))?;
    let cfg = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(cfg)
}

pub(crate) async fn with_loaded_remote_config<F, Fut>(alias: String, f: F) -> Result<i32>
where
    F: FnOnce(RemoteConfig) -> Fut,
    Fut: Future<Output = Result<i32>>,
{
    let cfg = load_remote_config(&alias)?;
    f(cfg).await
}

pub(crate) fn save_remote_config(cfg: &RemoteConfig) -> Result<()> {
    let path = config_path(&cfg.alias);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(cfg).context("serializing remote config")?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub(crate) fn ssh_args(cfg: &RemoteConfig) -> Vec<String> {
    let mut args = vec![
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

pub(crate) fn make_remote_step(
    id: &str,
    label: &str,
    detail: String,
    command: Option<String>,
    requires_network: bool,
    estimated_seconds: Option<u64>,
) -> RemoteStep {
    RemoteStep {
        id: id.into(),
        label: label.into(),
        detail,
        command,
        requires_network,
        estimated_seconds,
    }
}

pub(crate) fn build_remote_plan(
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
        make_remote_step(
            "preflight",
            "check local and remote prerequisites",
            "verify ssh, ssh-keygen, remote OS, docker, and systemd user support".into(),
            Some("ssh -p <port> host 'uname -s; uname -m; command -v docker; docker info; command -v systemctl; df -Pk $HOME'".into()),
            true,
            Some(2),
        ),
        make_remote_step(
            "binary",
            "upload the current binary",
            format!("stream {} to {}", current_exe_string(), cfg.remote_bin),
            Some(format!("ssh {} cat > {}", cfg.target, cfg.remote_bin)),
            true,
            Some(3),
        ),
        make_remote_step(
            "verify",
            "verify remote --version",
            "run the uploaded binary to confirm execution".into(),
            Some(format!("ssh {} {} --version", cfg.target, cfg.remote_bin)),
            true,
            Some(1),
        ),
    ];
    let service_detail = match opts.service_mode {
        ServiceMode::Auto => {
            "enable a user systemd unit when available, otherwise print manual serve guidance"
                .to_string()
        }
        ServiceMode::User => "force user systemd setup".to_string(),
        ServiceMode::Manual => "skip systemd and print manual serve guidance".to_string(),
    };
    steps.push(make_remote_step(
        "service",
        "configure the remote service",
        service_detail,
        Some("systemctl --user enable --now jeryu.service || print manual instructions".into()),
        true,
        Some(2),
    ));
    steps.push(make_remote_step(
        "save",
        "save the remote metadata",
        "write ~/.jeryu/remotes/<alias>.toml after verification".into(),
        Some(format!("write {}", config_path(&cfg.alias).display())),
        false,
        Some(1),
    ));
    RemoteInstallPlan {
        action: "remote-install".into(),
        connection: cfg.connection.clone(),
        options: opts.clone(),
        setup_key,
        preflight,
        steps,
    }
}

pub(crate) fn effective_service_mode(
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

pub(crate) async fn resolve_service_mode(cfg: &RemoteConfig) -> Result<ServiceMode> {
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

pub(crate) fn render_remote_plan(plan: &RemoteInstallPlan) {
    let color = should_colorize(plan.options.color, plan.options.json);
    println!(
        "{} {}",
        status_label(color, "PLAN", "36;1"),
        color_text(color, "1", "Remote install plan")
    );
    println!("  alias: {}", plan.connection.alias);
    println!("  target: {}", plan.connection.target);
    println!("  remote binary: {}", plan.connection.remote_bin);
    println!("  prefix: {}", plan.connection.remote_prefix);
    println!("  service mode: {:?}", plan.options.service_mode);
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
    render_plan_steps(
        &plan.steps,
        plan.options.verbose,
        |step| step.requires_network,
        |step| step.label.as_str(),
        |step| step.detail.as_str(),
        |step| step.command.as_deref(),
        color,
        "RUN",
        "OK",
        "36;1",
        "32;1",
    );
}

pub(crate) fn remote_confirmation(
    plan: &RemoteInstallPlan,
    opts: &RemoteCommonOptions,
) -> Result<bool> {
    render_remote_plan(plan);
    prompt_for_confirmation_with_message(
        "Proceed with remote install? [y/N] ",
        "refusing to mutate the remote host without --yes in non-interactive mode",
        opts.interactive,
        opts.yes,
    )
}

pub(crate) fn current_exe_string() -> String {
    match env::current_exe() {
        Ok(path) => path.display().to_string(),
        Err(_) => "(unavailable)".into(),
    }
}

pub(crate) fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
