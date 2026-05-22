use super::*;
use std::fs;

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
    let mut args: Vec<String> = Vec::new();
    // Only pass `-p` when the configured port differs from SSH's default (22).
    // Passing `-p 22` explicitly OVERRIDES any `Port` directive the user has
    // in ~/.ssh/config for this Host, which breaks setups where the target
    // alias resolves to a non-22 port via Host config. When `-p` is omitted,
    // ssh falls back to ~/.ssh/config's Port directive (or 22 if none).
    if cfg.ssh_port != 22 {
        args.push("-p".to_string());
        args.push(cfg.ssh_port.to_string());
    }
    args.extend([
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ControlMaster=auto".to_string(),
        "-o".to_string(),
        "ControlPersist=10m".to_string(),
        "-o".to_string(),
        "ControlPath=~/.ssh/jeryu-%r@%h:%p".to_string(),
    ]);
    if let Some(identity) = &cfg.identity {
        args.push("-i".to_string());
        args.push(identity.clone());
        // IdentitiesOnly=yes forces ssh to authenticate using ONLY the
        // -i key, never keys cached in an ssh-agent. Without this, ssh
        // tries agent keys first, exhausts the remote's MaxAuthTries
        // (default 6), and falls back to "Permission denied" before
        // ever offering our explicit key. The failure mode bit us in
        // CI where the GitHub Actions runner has agent keys preloaded.
        args.push("-o".to_string());
        args.push("IdentitiesOnly=yes".to_string());
    }
    args
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub(super) fn ssh_bash_command(cfg: &RemoteConfig, script: &str) -> Command {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    // SSH joins all remaining args with spaces and passes them as ONE shell
    // command string to the remote sh. If we send `bash -c <script>` as
    // three args, the remote sh sees:
    //     bash -c <first-word-of-script> <rest-positional-args>
    // — bash only treats the first word as the command. So a script like
    // `mkdir -p $HOME/x && cat > $HOME/y` runs `mkdir` with no operands.
    // Wrap the whole `bash -c <script>` in a single argument with the
    // script quoted, so the remote shell re-parses it as one bash invocation.
    //
    // We use `-c` (not `-lc`): the prior `-l` (login shell) sourced
    // /etc/profile on the remote, which on some images triggers init
    // logic (e.g., the docker-installer's profile fragment can launch
    // a docker compose pull). We don't need login mode — these scripts
    // either set their own env (uploads, install) or invoke a self-
    // contained binary that needs no shell init.
    cmd.arg(format!("bash -c {}", shell_single_quote(script)));
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

#[path = "remote_support_plan.rs"]
mod plan;
pub(crate) use plan::*;
