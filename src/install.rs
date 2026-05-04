//! Owner: Local installer and guided bootstrap UX
//! Proof: `cargo test -p jeryu -- install`
//! Invariants: Local installs remain user-space by default, avoid shell mutations unless requested, and never require sudo for the default path.

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use chrono::Utc;
use serde::Serialize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tempfile::Builder;
use tokio::process::Command;

use crate::install_demo;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum InteractiveMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum PathMode {
    Advise,
    Update,
    Skip,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformProbe {
    pub os: String,
    pub arch: String,
    pub shell: Option<String>,
    pub tty: bool,
    pub in_path: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathAdvice {
    pub shell: Option<String>,
    pub rc_file: Option<String>,
    pub snippet: Option<String>,
    pub update_performed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub action: String,
    pub mode: String,
    pub prefix: String,
    pub target_binary: String,
    pub source_binary: String,
    pub platform: PlatformProbe,
    pub path_advice: Option<PathAdvice>,
    pub dry_run: bool,
    pub json: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub path_mode: PathMode,
    pub verbose: bool,
    pub install_deps: bool,
    pub allow_sudo: bool,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStep {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub command: Option<String>,
    pub requires_sudo: bool,
    pub estimated_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub prefix: String,
    pub binary: String,
    pub current_exe: String,
    pub installed: bool,
    pub version_ok: bool,
    pub version_output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub prefix: PathBuf,
    pub dry_run: bool,
    pub json: bool,
    pub yes: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub path_mode: PathMode,
    pub verbose: bool,
    pub install_deps: bool,
    pub allow_sudo: bool,
}

#[derive(Debug, Clone)]
pub enum InstallAction {
    Local,
    Doctor,
    Smoke,
    Server,
    Uninstall,
    RenderDemo {
        output: PathBuf,
        png: Option<PathBuf>,
    },
}

pub async fn execute_install(action: Option<InstallAction>, opts: InstallOptions) -> Result<i32> {
    match action.unwrap_or(InstallAction::Local) {
        InstallAction::Local => install_local(&opts).await,
        InstallAction::Doctor => doctor(&opts).await,
        InstallAction::Smoke => smoke(&opts).await,
        InstallAction::Server => server(&opts).await,
        InstallAction::Uninstall => uninstall(&opts).await,
        InstallAction::RenderDemo { output, png } => {
            if opts.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "action": "render-demo",
                        "output": output,
                        "png": png,
                        "dry_run": opts.dry_run,
                    }))?
                );
            }
            if opts.dry_run {
                println!(
                    "dry-run: would render install demo GIF to {}",
                    output.display()
                );
                return Ok(0);
            }
            install_demo::render_install_demo(&install_demo::Args { output, png })?;
            println!("install demo rendered");
            Ok(0)
        }
    }
}

fn current_exe_string() -> String {
    env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "(unavailable)".into())
}

pub fn expand_tilde(input: impl AsRef<str>) -> PathBuf {
    let input = input.as_ref();
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(input)
}

fn install_target(prefix: &Path) -> PathBuf {
    prefix.join("jeryu")
}

fn detect_platform(prefix: &Path) -> PlatformProbe {
    let shell = env::var("SHELL").ok();
    PlatformProbe {
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        shell,
        tty: io::stdout().is_terminal(),
        in_path: path_contains_dir(prefix),
    }
}

fn path_contains_dir(dir: &Path) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|entry| entry == dir)
}

fn shell_profile_path(shell: Option<&str>) -> Option<PathBuf> {
    let shell = shell?;
    let name = Path::new(shell).file_name()?.to_string_lossy().to_ascii_lowercase();
    let home = dirs::home_dir()?;
    match name.as_str() {
        "bash" => Some(home.join(".bashrc")),
        "zsh" => Some(home.join(".zshrc")),
        "fish" => Some(home.join(".config/fish/config.fish")),
        _ => None,
    }
}

fn path_snippet(prefix: &Path, shell: Option<&str>) -> String {
    let path = prefix.display();
    match shell
        .map(|value| Path::new(value).file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("fish") => format!(
            "# >>> jeryu path >>>\nset -gx PATH \"{}\" $PATH\n# <<< jeryu path <<<",
            path
        ),
        _ => format!(
            "# >>> jeryu path >>>\nexport PATH=\"{}:$PATH\"\n# <<< jeryu path <<<",
            path
        ),
    }
}

fn build_plan(mode: &str, opts: &InstallOptions) -> InstallPlan {
    let prefix = opts.prefix.display().to_string();
    let target = install_target(&opts.prefix);
    let source = current_exe_string();
    let platform = detect_platform(&opts.prefix);
    let path_advice = if platform.in_path {
        None
    } else {
        let rc_file = shell_profile_path(platform.shell.as_deref());
        Some(PathAdvice {
            shell: platform.shell.clone(),
            rc_file: rc_file.as_ref().map(|path| path.display().to_string()),
            snippet: rc_file
                .as_ref()
                .map(|_| path_snippet(&opts.prefix, platform.shell.as_deref())),
            update_performed: matches!(opts.path_mode, PathMode::Update),
        })
    };
    let mut steps = vec![
        PlanStep {
            id: "ensure-prefix".into(),
            label: "ensure install prefix exists".into(),
            detail: format!("create {}", opts.prefix.display()),
            command: Some(format!("mkdir -p {}", opts.prefix.display())),
            requires_sudo: false,
            estimated_seconds: Some(1),
        },
        PlanStep {
            id: "install-binary".into(),
            label: "replace the binary atomically".into(),
            detail: format!("copy {} -> {}", source, target.display()),
            command: Some(format!("install -m 0755 <current-exe> {}", target.display())),
            requires_sudo: false,
            estimated_seconds: Some(2),
        },
    ];
    if !platform.in_path {
        let detail = match opts.path_mode {
            PathMode::Advise => "print shell-specific PATH advice".to_string(),
            PathMode::Update => "update the shell profile with a guarded PATH block".to_string(),
            PathMode::Skip => "skip PATH advice and leave shell profiles untouched".to_string(),
        };
        steps.push(PlanStep {
            id: "path".into(),
            label: "handle PATH visibility".into(),
            detail,
            command: Some(match opts.path_mode {
                PathMode::Advise => format!(
                    "echo {}",
                    path_snippet(&opts.prefix, platform.shell.as_deref())
                ),
                PathMode::Update => {
                    if let Some(rc) = shell_profile_path(platform.shell.as_deref()) {
                        format!("append {} to {}", opts.prefix.display(), rc.display())
                    } else {
                        "no supported shell profile found".into()
                    }
                }
                PathMode::Skip => "no PATH mutation".into(),
            }),
            requires_sudo: false,
            estimated_seconds: Some(1),
        });
    }
    steps.push(PlanStep {
        id: "verify".into(),
        label: "verify the installed binary".into(),
        detail: "run jeryu --version from the target binary".into(),
        command: Some(format!("{} --version", target.display())),
        requires_sudo: false,
        estimated_seconds: Some(1),
    });
    InstallPlan {
        action: "install".into(),
        mode: mode.into(),
        prefix,
        target_binary: target.display().to_string(),
        source_binary: source,
        platform,
        path_advice,
        dry_run: opts.dry_run,
        json: opts.json,
        color: opts.color,
        interactive: opts.interactive,
        path_mode: opts.path_mode,
        verbose: opts.verbose,
        install_deps: opts.install_deps,
        allow_sudo: opts.allow_sudo,
        steps,
    }
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
        InteractiveMode::Auto => io::stdout().is_terminal(),
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

fn render_plan(plan: &InstallPlan) {
    let color = should_colorize(plan.color, plan.json);
    println!(
        "{} {}",
        status_label(color, "PLAN", "36;1"),
        color_text(color, "1", &format!("JeRyu {} plan", plan.mode))
    );
    println!("  prefix: {}", plan.prefix);
    println!("  target: {}", plan.target_binary);
    println!("  source: {}", plan.source_binary);
    println!(
        "  platform: {} / {}{}",
        plan.platform.os,
        plan.platform.arch,
        if plan.platform.tty { " / tty" } else { "" }
    );
    println!("  PATH: {}", if plan.platform.in_path { "already on PATH" } else { "not on PATH" });
    for step in &plan.steps {
        let label = if step.requires_sudo {
            status_label(color, "WARN", "33;1")
        } else {
            status_label(color, "RUN", "36;1")
        };
        println!("  {} {} - {}", label, step.label, step.detail);
        if plan.verbose && let Some(command) = &step.command {
            println!("      {}", command);
        }
    }
    if let Some(advice) = &plan.path_advice {
        match plan.path_mode {
            PathMode::Skip => {
                println!("  PATH: skipped by request");
            }
            PathMode::Advise | PathMode::Update => {
                if let Some(snippet) = &advice.snippet {
                    println!("  PATH snippet:");
                    for line in snippet.lines() {
                        println!("      {}", line);
                    }
                }
            }
        }
    }
}

fn prompt_for_confirmation(_plan: &InstallPlan, opts: &InstallOptions) -> Result<bool> {
    if opts.yes {
        return Ok(true);
    }
    if !should_interactive(opts.interactive) {
        bail!(
            "refusing to mutate the machine without --yes in non-interactive mode; rerun with --yes or --dry-run"
        );
    }
    print!("Proceed with this install? [y/N] ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading confirmation")?;
    Ok(matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn friendly_retry(binary: &Path) -> String {
    format!("Try: {} --version", binary.display())
}

async fn install_local(opts: &InstallOptions) -> Result<i32> {
    let plan = build_plan("local", opts);
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_plan(&plan);
    }
    if opts.dry_run {
        return Ok(0);
    }

    if matches!(opts.path_mode, PathMode::Update)
        && !plan.platform.in_path
        && shell_profile_path(plan.platform.shell.as_deref()).is_none()
    {
        bail!(
            "--path-mode update requires a supported shell profile (bash, zsh, or fish)"
        );
    }
    if !prompt_for_confirmation(&plan, opts)? {
        bail!("install cancelled");
    }

    let step_started = Instant::now();
    install_binary(&opts.prefix).await?;
    if matches!(opts.path_mode, PathMode::Update) {
        update_shell_profile(&opts.prefix, plan.platform.shell.as_deref())?;
    }
    verify_binary(&install_target(&opts.prefix)).await?;
    if !plan.platform.in_path && matches!(opts.path_mode, PathMode::Advise) {
        if let Some(advice) = &plan.path_advice {
            if let Some(rc) = &advice.rc_file {
                println!("PATH advice: add {} to {}", opts.prefix.display(), rc);
            }
            if let Some(snippet) = &advice.snippet {
                println!("{snippet}");
            }
        }
    }
    println!(
        "{} installed jeryu to {} in {}s",
        status_label(should_colorize(opts.color, opts.json), "OK", "32;1"),
        install_target(&opts.prefix).display(),
        step_started.elapsed().as_secs_f32()
    );
    Ok(0)
}

async fn doctor(opts: &InstallOptions) -> Result<i32> {
    let target = install_target(&opts.prefix);
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("(unavailable)"));
    let version = match run_output(&target, &["--version"]).await {
        Ok(output) => Some(output.trim().to_string()),
        Err(_) => None,
    };
    let report = DoctorReport {
        prefix: opts.prefix.display().to_string(),
        binary: target.display().to_string(),
        current_exe: current_exe.display().to_string(),
        installed: target.exists(),
        version_ok: version.is_some(),
        version_output: version,
    };
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("JeRyu install doctor");
        println!("  prefix:       {}", report.prefix);
        println!("  binary:       {}", report.binary);
        println!("  current exe:  {}", report.current_exe);
        println!("  installed:    {}", report.installed);
        println!("  version ok:   {}", report.version_ok);
        if let Some(output) = &report.version_output {
            println!("  version:      {}", output);
        }
    }
    if !report.installed {
        bail!("installed binary not found: {}", report.binary);
    }
    if !report.version_ok {
        bail!("installed binary did not respond to --version: {}", friendly_retry(&target));
    }
    Ok(0)
}

async fn smoke(opts: &InstallOptions) -> Result<i32> {
    let tmp = tempfile::tempdir().context("creating smoke temp dir")?;
    let smoke_opts = InstallOptions {
        prefix: tmp.path().to_path_buf(),
        dry_run: opts.dry_run,
        json: opts.json,
        yes: opts.yes,
        color: opts.color,
        interactive: opts.interactive,
        path_mode: opts.path_mode,
        verbose: opts.verbose,
        install_deps: false,
        allow_sudo: false,
    };
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "smoke",
                "prefix": smoke_opts.prefix,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("JeRyu install smoke");
    }
    if opts.dry_run {
        return Ok(0);
    }
    if !prompt_for_confirmation(&build_plan("smoke", &smoke_opts), &smoke_opts)? {
        bail!("smoke install cancelled");
    }
    install_binary(&smoke_opts.prefix).await?;
    verify_binary(&install_target(&smoke_opts.prefix)).await?;
    Ok(0)
}

async fn server(opts: &InstallOptions) -> Result<i32> {
    let prefix = &opts.prefix;
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "server",
                "prefix": prefix,
                "dry_run": opts.dry_run,
                "install_deps": opts.install_deps,
                "allow_sudo": opts.allow_sudo,
            }))?
        );
    } else {
        println!("JeRyu server setup");
    }
    if opts.dry_run {
        return Ok(0);
    }

    if !prompt_for_confirmation(&build_plan("server", opts), opts)? {
        bail!("server setup cancelled");
    }
    install_binary(prefix).await?;
    ensure_docker(opts).await?;
    run_installed_binary(&install_target(prefix), &["init"]).await?;
    Ok(0)
}

async fn uninstall(opts: &InstallOptions) -> Result<i32> {
    let target = install_target(&opts.prefix);
    let backup_prefix = opts.prefix.join(".jeryu-backups");
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "uninstall",
                "prefix": opts.prefix,
                "binary": target,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("JeRyu uninstall");
    }
    if opts.dry_run {
        return Ok(0);
    }
    if target.exists() {
        fs::remove_file(&target).with_context(|| format!("removing {}", target.display()))?;
    }
    if backup_prefix.exists() {
        let _ = fs::remove_dir_all(&backup_prefix);
    }
    Ok(0)
}

async fn install_binary(prefix: &Path) -> Result<()> {
    fs::create_dir_all(prefix).with_context(|| format!("creating {}", prefix.display()))?;
    let source = std::env::current_exe().context("locating current executable")?;
    let target = install_target(prefix);
    let tmp = Builder::new()
        .prefix("jeryu-install-")
        .tempfile_in(prefix)
        .with_context(|| format!("creating temp file in {}", prefix.display()))?;
    let tmp_path = tmp.path().to_path_buf();
    fs::copy(&source, &tmp_path)
        .with_context(|| format!("copying {} -> {}", source.display(), tmp_path.display()))?;
    set_executable(&tmp_path)?;
    if target.exists() {
        let backup_dir = prefix.join(".jeryu-backups");
        fs::create_dir_all(&backup_dir)
            .with_context(|| format!("creating {}", backup_dir.display()))?;
        let backup = backup_dir.join(format!("jeryu-{}.bak", Utc::now().format("%Y%m%d%H%M%S")));
        fs::copy(&target, &backup)
            .with_context(|| format!("backing up {} -> {}", target.display(), backup.display()))?;
    }
    fs::rename(&tmp_path, &target).with_context(|| {
        format!(
            "atomically replacing {} with {}",
            target.display(),
            tmp_path.display()
        )
    })?;
    drop(tmp);
    verify_binary(&target).await?;
    Ok(())
}

async fn verify_binary(target: &Path) -> Result<()> {
    run_output(target, &["--version"])
        .await
        .with_context(|| format!("verifying {}", target.display()))?;
    Ok(())
}

async fn ensure_docker(opts: &InstallOptions) -> Result<()> {
    if run_status("docker", &["info"]).await.is_ok() {
        return Ok(());
    }
    if !opts.install_deps {
        bail!(
            "docker is required for --server; rerun with --install-deps --allow-sudo to install it"
        );
    }
    if !opts.allow_sudo {
        bail!("--install-deps requires --allow-sudo to install docker packages");
    }
    install_docker_packages().await
}

async fn install_docker_packages() -> Result<()> {
    if command_exists("apt-get").await {
        run_privileged("apt-get", &["update"]).await?;
        run_privileged(
            "apt-get",
            &["install", "-y", "docker.io", "docker-compose-plugin"],
        )
        .await?;
        return Ok(());
    }
    if command_exists("dnf").await {
        run_privileged("dnf", &["install", "-y", "docker", "docker-compose-plugin"]).await?;
        return Ok(());
    }
    if command_exists("yum").await {
        run_privileged("yum", &["install", "-y", "docker", "docker-compose-plugin"]).await?;
        return Ok(());
    }
    if command_exists("zypper").await {
        run_privileged(
            "zypper",
            &[
                "--non-interactive",
                "install",
                "docker",
                "docker-compose-plugin",
            ],
        )
        .await?;
        return Ok(());
    }
    if command_exists("pacman").await {
        run_privileged(
            "pacman",
            &["-Sy", "--noconfirm", "docker", "docker-compose"],
        )
        .await?;
        return Ok(());
    }
    if command_exists("apk").await {
        run_privileged("apk", &["add", "docker", "docker-cli", "docker-compose"]).await?;
        return Ok(());
    }
    bail!("unable to install docker automatically on this host");
}

async fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-lc")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn run_privileged(cmd: &str, args: &[&str]) -> Result<()> {
    if is_root() {
        run_status(cmd, args).await
    } else {
        let mut prefixed: Vec<&str> = Vec::with_capacity(args.len() + 1);
        prefixed.push(cmd);
        prefixed.extend_from_slice(args);
        run_status("sudo", &prefixed).await
    }
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

async fn run_installed_binary(target: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new(target)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("running {}", target.display()))?;
    if !status.success() {
        bail!(
            "{} exited with {}",
            target.display(),
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".into())
        );
    }
    Ok(())
}

fn update_shell_profile(prefix: &Path, shell: Option<&str>) -> Result<()> {
    let Some(rc_path) = shell_profile_path(shell) else {
        bail!("--path-mode update requires a supported shell (bash, zsh, or fish)");
    };
    let snippet = path_snippet(prefix, shell);
    let existing = fs::read_to_string(&rc_path).unwrap_or_default();
    if existing.contains("# >>> jeryu path >>>") {
        return Ok(());
    }
    if let Some(parent) = rc_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    if rc_path.exists() {
        let backup = rc_path.with_extension("bak");
        fs::copy(&rc_path, &backup)
            .with_context(|| format!("backing up {} -> {}", rc_path.display(), backup.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_path)
        .with_context(|| format!("opening {}", rc_path.display()))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{}", snippet)?;
    Ok(())
}

async fn run_output(target: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(target)
        .args(args)
        .output()
        .await
        .with_context(|| format!("running {}", target.display()))?;
    if !output.status.success() {
        bail!(
            "{} exited with {}",
            target.display(),
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".into())
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_status(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .with_context(|| format!("running {} {}", cmd, args.join(" ")))?;
    if status.success() {
        Ok(())
    } else {
        bail!(
            "{} {} exited with {}",
            cmd,
            args.join(" "),
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".into())
        );
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tilde_expansion_targets_home() {
        let prefix = expand_tilde("~/.jeryu/bin");
        assert!(prefix.ends_with(".jeryu/bin"));
    }

    #[test]
    fn install_plan_stays_user_space() {
        let plan = build_plan(
            "local",
            &InstallOptions {
                prefix: "/tmp/jeryu".into(),
                dry_run: true,
                json: true,
                yes: true,
                color: ColorMode::Never,
                interactive: InteractiveMode::Never,
                path_mode: PathMode::Advise,
                verbose: false,
                install_deps: false,
                allow_sudo: false,
            },
        );
        let rendered = serde_json::to_value(&plan).unwrap();
        let steps = rendered["steps"].as_array().unwrap();
        assert_eq!(rendered["mode"], "local");
        assert!(!rendered["install_deps"].as_bool().unwrap());
        assert!(!rendered["allow_sudo"].as_bool().unwrap());
        assert!(steps.iter().all(|step| {
            let label = step["label"].as_str().unwrap();
            let command = step["command"].as_str().unwrap();
            !label.contains("sudo")
                && !label.contains("python")
                && !label.contains("pip")
                && !command.contains("sudo")
                && !command.contains("python")
                && !command.contains("pip")
        }));
    }

    #[test]
    fn path_snippets_are_shell_specific() {
        assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/bash")).contains("export PATH"));
        assert!(path_snippet(Path::new("/tmp/bin"), Some("/usr/bin/fish")).contains("set -gx PATH"));
    }

    #[test]
    fn plan_tracks_path_advice_for_unknown_prefix() {
        let plan = build_plan(
            "local",
            &InstallOptions {
                prefix: tempdir().unwrap().path().join("jeryu-bin"),
                dry_run: true,
                json: false,
                yes: true,
                color: ColorMode::Auto,
                interactive: InteractiveMode::Auto,
                path_mode: PathMode::Advise,
                verbose: false,
                install_deps: false,
                allow_sudo: false,
            },
        );
        assert!(plan.path_advice.is_some());
        assert!(plan.steps.iter().any(|step| step.id == "verify"));
    }

    #[test]
    fn color_mode_respects_never_and_always() {
        assert!(!should_colorize(ColorMode::Never, false));
        assert!(should_colorize(ColorMode::Always, false));
    }
}
