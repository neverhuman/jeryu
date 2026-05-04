use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::Builder;
use tokio::process::Command;

use crate::install_demo;

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub action: String,
    pub prefix: String,
    pub source_binary: String,
    pub dry_run: bool,
    pub json: bool,
    pub install_deps: bool,
    pub allow_sudo: bool,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStep {
    pub label: String,
    pub command: Option<String>,
    pub requires_sudo: bool,
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

async fn install_local(opts: &InstallOptions) -> Result<i32> {
    let plan = InstallPlan {
        action: "install".into(),
        prefix: opts.prefix.display().to_string(),
        source_binary: std::env::current_exe()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "(unavailable)".into()),
        dry_run: opts.dry_run,
        json: opts.json,
        install_deps: false,
        allow_sudo: false,
        steps: vec![
            PlanStep {
                label: "ensure prefix exists".into(),
                command: Some(format!("mkdir -p {}", opts.prefix.display())),
                requires_sudo: false,
            },
            PlanStep {
                label: "copy current binary".into(),
                command: Some(format!(
                    "install -m 0755 <current-exe> {}",
                    install_target(&opts.prefix).display()
                )),
                requires_sudo: false,
            },
            PlanStep {
                label: "verify jeryu --version".into(),
                command: Some(format!(
                    "{} --version",
                    install_target(&opts.prefix).display()
                )),
                requires_sudo: false,
            },
        ],
    };

    if opts.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("JeRyu install plan");
        println!("  prefix: {}", plan.prefix);
        for step in &plan.steps {
            println!("  - {}", step.label);
        }
    }
    if opts.dry_run {
        return Ok(0);
    }

    install_binary(&opts.prefix).await?;
    verify_binary(&install_target(&opts.prefix)).await?;
    println!(
        "installed jeryu to {}",
        install_target(&opts.prefix).display()
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
        bail!("installed binary did not respond to --version");
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

    #[test]
    fn tilde_expansion_targets_home() {
        let prefix = expand_tilde("~/.jeryu/bin");
        assert!(prefix.ends_with(".jeryu/bin"));
    }

    #[test]
    fn install_plan_stays_user_space() {
        let plan = InstallPlan {
            action: "install".into(),
            prefix: "/tmp/jeryu".into(),
            source_binary: "/tmp/current".into(),
            dry_run: true,
            json: true,
            install_deps: false,
            allow_sudo: false,
            steps: vec![PlanStep {
                label: "copy current binary".into(),
                command: Some("install -m 0755 <current-exe> /tmp/jeryu/jeryu".into()),
                requires_sudo: false,
            }],
        };
        let rendered = serde_json::to_value(&plan).unwrap();
        let steps = rendered["steps"].as_array().unwrap();
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
}
