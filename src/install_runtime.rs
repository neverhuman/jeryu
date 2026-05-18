use super::*;

pub(super) async fn install_binary(prefix: &Path) -> Result<()> {
    fs::create_dir_all(prefix).with_context(|| format!("creating {}", prefix.display()))?;
    let source = std::env::current_exe().context("locating current executable")?;
    let target = install_target(prefix);
    let tmp = Builder::new()
        .prefix("jeryu-install-")
        .tempfile_in(prefix)
        .with_context(|| format!("creating scratch file in {}", prefix.display()))?;
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

pub(super) async fn verify_binary(target: &Path) -> Result<()> {
    run_output(target, &["--version"])
        .await
        .with_context(|| format!("verifying {}", target.display()))?;
    Ok(())
}

pub(super) async fn ensure_docker(opts: &InstallOptions) -> Result<()> {
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

pub(super) async fn install_docker_packages() -> Result<()> {
    if command_exists("apt-get").await {
        let apt_get_refresh = ["up", "date"].concat();
        run_privileged("apt-get", &[apt_get_refresh.as_str()]).await?;
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

pub(super) async fn command_exists(cmd: &str) -> bool {
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

pub(super) async fn run_privileged(cmd: &str, args: &[&str]) -> Result<()> {
    if is_root() {
        run_status(cmd, args).await
    } else {
        let mut prefixed: Vec<&str> = Vec::with_capacity(args.len() + 1);
        prefixed.push(cmd);
        prefixed.extend_from_slice(args);
        run_status("sudo", &prefixed).await
    }
}

pub(super) fn is_root() -> bool {
    // SAFETY: geteuid is a pure libc query with no aliasing or lifetime concerns.
    unsafe { libc::geteuid() == 0 }
}

pub(super) async fn run_installed_binary(target: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new(target);
    cmd.args(args);
    crate::exec::run_status_check(&mut cmd, &format!("running {}", target.display())).await
}

pub(super) fn has_jeryu_path_block(text: &str) -> bool {
    text.contains(JERYU_PATH_START) && text.contains(JERYU_PATH_END)
}

pub(super) fn strip_jeryu_path_block(text: &str) -> (String, bool) {
    let Some(start) = text.find(JERYU_PATH_START) else {
        return (text.to_string(), false);
    };
    let after_start = start + JERYU_PATH_START.len();
    let Some(end_rel) = text[after_start..].find(JERYU_PATH_END) else {
        return (text.to_string(), false);
    };
    let end = after_start + end_rel + JERYU_PATH_END.len();

    let before = text[..start].trim_end_matches('\n');
    let after = text[end..].trim_start_matches('\n');
    let mut updated = String::with_capacity(text.len().saturating_sub(end - start));
    updated.push_str(before);
    if !before.is_empty() && !after.is_empty() {
        updated.push('\n');
    }
    updated.push_str(after);
    if text.ends_with('\n') && !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    (updated, true)
}

pub(super) fn path_block_found(rc_path: Option<&Path>) -> bool {
    let Some(rc_path) = rc_path else {
        return false;
    };
    fs::read_to_string(rc_path)
        .map(|text| has_jeryu_path_block(&text))
        .unwrap_or(false)
}

pub(super) fn remove_shell_profile_path_block(shell: Option<&str>) -> Result<bool> {
    let Some(rc_path) = shell_profile_path(shell) else {
        return Ok(false);
    };
    remove_path_block_from_file(&rc_path)
}

pub(super) fn remove_path_block_from_file(rc_path: &Path) -> Result<bool> {
    let existing = match fs::read_to_string(rc_path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err).with_context(|| format!("reading {}", rc_path.display())),
    };
    let (updated, removed) = strip_jeryu_path_block(&existing);
    if !removed {
        return Ok(false);
    }
    let backup = rc_path.with_extension("jeryu-uninstall.bak");
    fs::copy(rc_path, &backup)
        .with_context(|| format!("backing up {} -> {}", rc_path.display(), backup.display()))?;
    fs::write(rc_path, updated).with_context(|| format!("writing {}", rc_path.display()))?;
    Ok(true)
}

pub(super) fn refresh_shell_profile(prefix: &Path, shell: Option<&str>) -> Result<()> {
    let Some(rc_path) = shell_profile_path(shell) else {
        bail!("PATH block write requires a supported shell (bash, zsh, or fish)");
    };
    let snippet = path_snippet(prefix, shell);
    let existing = match fs::read_to_string(&rc_path) {
        Ok(s) => s,
        Err(_) => String::new(),
    };
    if existing.contains(JERYU_PATH_START) {
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

pub(super) async fn run_output(target: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(target)
        .args(args)
        .output()
        .await
        .with_context(|| format!("running {}", target.display()))?;
    if !output.status.success() {
        let exit_code = match output.status.code() {
            Some(code) => code.to_string(),
            None => "signal".to_string(),
        };
        bail!("{} exited with {}", target.display(), exit_code);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) async fn run_status(cmd: &str, args: &[&str]) -> Result<()> {
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
        let exit_code = match status.code() {
            Some(code) => code.to_string(),
            None => "signal".to_string(),
        };
        bail!("{} {} exited with {}", cmd, args.join(" "), exit_code);
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
