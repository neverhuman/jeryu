use super::*;

pub(crate) async fn command_exists(cmd: &str) -> bool {
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

pub(crate) async fn run_privileged(cmd: &str, args: &[&str]) -> Result<()> {
    if is_root() {
        run_status(cmd, args).await
    } else {
        let mut prefixed: Vec<&str> = Vec::with_capacity(args.len() + 1);
        prefixed.push(cmd);
        prefixed.extend_from_slice(args);
        run_status("sudo", &prefixed).await
    }
}

pub(crate) fn is_root() -> bool {
    // SAFETY: geteuid is a pure libc query with no aliasing or lifetime concerns.
    unsafe { libc::geteuid() == 0 }
}

pub(crate) async fn run_installed_binary(target: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new(target);
    cmd.args(args);
    crate::exec::run_status_check(&mut cmd, &format!("running {}", target.display())).await
}

pub(crate) async fn run_output(target: &Path, args: &[&str]) -> Result<String> {
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

pub(crate) async fn run_status(cmd: &str, args: &[&str]) -> Result<()> {
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
