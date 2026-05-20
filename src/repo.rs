use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub async fn install_git_hooks() -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_git_hooks(&repo_root)?;
    println!("Configured local core.hooksPath to ops/git-hooks");
    Ok(0)
}

pub async fn state_proof() -> Result<i32> {
    let redlinedb_bin = redlinedb_bin_path();
    validate_redlinedb_bin(&redlinedb_bin)?;

    let version = Command::new(&redlinedb_bin)
        .arg("--version")
        .output()
        .await
        .with_context(|| {
            format!(
                "running {} --version; install or symlink RedlineDB at {}",
                redlinedb_bin.display(),
                default_redlinedb_bin_path().display()
            )
        })?;
    if !version.status.success() {
        bail!(
            "{} --version failed: {}. Install or symlink RedlineDB at {}",
            redlinedb_bin.display(),
            String::from_utf8_lossy(&version.stderr).trim(),
            default_redlinedb_bin_path().display()
        );
    }

    let proof_dir =
        std::env::temp_dir().join(format!("jeryu-redline-proof-{}", std::process::id()));
    fs::create_dir_all(&proof_dir).with_context(|| format!("creating {}", proof_dir.display()))?;
    let proof_db = proof_dir.join("state-proof.redlineDB");
    let url = format!("redline:{}?mode=rwc", proof_db.display());

    let mut test = Command::new("cargo");
    test.args([
        "test",
        "-p",
        "jeryu",
        "state::tests::redline_backend_smoke_test_when_configured",
        "--",
        "--nocapture",
    ]);
    test.env("JERYU_TEST_REDLINE_URL", &url);
    let result = crate::exec::run_status_check(&mut test, "running redline proof test").await;
    if std::env::var("JERYU_KEEP_REDLINE_PROOF").ok().as_deref() != Some("1") {
        let _ = fs::remove_dir_all(&proof_dir);
    }
    result?;
    Ok(0)
}

pub(crate) fn configure_git_hooks(repo_root: &Path) -> Result<()> {
    let hooks_dir = repo_root.join("ops/git-hooks");
    let pre_push = hooks_dir.join("pre-push");
    if !pre_push.is_file() {
        bail!("repo-managed hook is missing: {}", pre_push.display());
    }

    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "core.hooksPath", "ops/git-hooks"])
        .output()
        .with_context(|| "configuring repo-managed git hooks".to_string())?;

    if !output.status.success() {
        bail!(
            "failed to configure core.hooksPath: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

fn redlinedb_bin_path() -> PathBuf {
    std::env::var_os("REDLINEDB_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(default_redlinedb_bin_path)
}

fn default_redlinedb_bin_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/ubuntu"))
        .join(".local/bin/redlinedb")
}

fn validate_redlinedb_bin(redlinedb_bin: &Path) -> Result<()> {
    if !redlinedb_bin.is_file() {
        bail!(
            "required RedlineDB binary is missing: {}. Install or symlink RedlineDB at {}",
            redlinedb_bin.display(),
            default_redlinedb_bin_path().display()
        );
    }

    if !is_executable(redlinedb_bin) {
        bail!(
            "required RedlineDB binary is not executable: {}. Install or symlink RedlineDB at {}",
            redlinedb_bin.display(),
            default_redlinedb_bin_path().display()
        );
    }

    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub async fn capture_tui_screenshots(output_dir: Option<PathBuf>) -> Result<i32> {
    let root = repo_root()?;
    let output_dir = match output_dir {
        Some(path) => path,
        None => root.join("paper/assets"),
    };
    let debug_dir = root.join("target/tui-capture");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating {}", output_dir.display()))?;
    fs::create_dir_all(&debug_dir).with_context(|| format!("creating {}", debug_dir.display()))?;

    let cols = std::env::var("JERYU_TUI_CAPTURE_COLS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(160);
    let rows = std::env::var("JERYU_TUI_CAPTURE_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(48);
    let font_path = match std::env::var("JERYU_TUI_CAPTURE_FONT") {
        Ok(value) => PathBuf::from(value),
        Err(_) => PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    };
    let font_size = std::env::var("JERYU_TUI_CAPTURE_FONT_SIZE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(19.0);
    let cell_w = std::env::var("JERYU_TUI_CAPTURE_CELL_W")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);
    let cell_h = std::env::var("JERYU_TUI_CAPTURE_CELL_H")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(23);
    let bg = match std::env::var("JERYU_TUI_CAPTURE_BG") {
        Ok(value) => value,
        Err(_) => "#17212b".to_string(),
    };
    let fg = match std::env::var("JERYU_TUI_CAPTURE_FG") {
        Ok(value) => value,
        Err(_) => "#f4fbff".to_string(),
    };
    let brighten = std::env::var("JERYU_TUI_CAPTURE_BRIGHTEN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1.35_f32);
    let max_wait_ms = std::env::var("JERYU_TUI_CAPTURE_MAX_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8000_u64);
    let min_wait_ms = std::env::var("JERYU_TUI_CAPTURE_MIN_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1200_u64);
    let quiet_ms = std::env::var("JERYU_TUI_CAPTURE_QUIET_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300_u64);

    let mut build = Command::new("cargo");
    build
        .args(["build", "--release", "-p", "jeryu", "-p", "tui-capture"])
        .current_dir(&root);
    crate::exec::run_status_check(&mut build, "building tui-capture assets").await?;

    let shots = [
        ("mission", output_dir.join("jeryu-tui-mission.png")),
        ("jobs", output_dir.join("jeryu-tui-jobs-flow.png")),
        ("agents", output_dir.join("jeryu-tui-agents.png")),
        ("tests", output_dir.join("jeryu-tui-tests-vti.png")),
        ("evidence", output_dir.join("jeryu-tui-evidence.png")),
        ("release", output_dir.join("jeryu-tui-release.png")),
    ];

    for (tab, output) in shots {
        let ready_file = tempfile::NamedTempFile::new().context("creating ready file")?;
        let mut cmd = Command::new(root.join("target/release/tui-capture"));
        cmd.arg("--cols").arg(cols.to_string());
        cmd.arg("--rows").arg(rows.to_string());
        cmd.arg("--out").arg(&output);
        cmd.arg("--font").arg(&font_path);
        cmd.arg("--font-size").arg(font_size.to_string());
        cmd.arg("--cell-w").arg(cell_w.to_string());
        cmd.arg("--cell-h").arg(cell_h.to_string());
        cmd.arg("--bg").arg(&bg);
        cmd.arg("--fg").arg(&fg);
        cmd.arg("--brighten").arg(brighten.to_string());
        cmd.arg("--min-wait-ms").arg(min_wait_ms.to_string());
        cmd.arg("--max-wait-ms").arg(max_wait_ms.to_string());
        cmd.arg("--quiet-ms").arg(quiet_ms.to_string());
        cmd.arg("--ready-file").arg(ready_file.path());
        cmd.arg("--dump-text")
            .arg(debug_dir.join(format!("{tab}.txt")));
        cmd.arg("--");
        cmd.arg(root.join("target/release/jeryu"));
        cmd.arg("tui");
        cmd.arg("--screenshot");
        cmd.arg("--tab").arg(tab);
        cmd.arg("--screenshot-hold-ms").arg("10000");
        crate::exec::run_status_check(&mut cmd, &format!("tui capture failed for {tab}")).await?;
        if !ready_file.path().exists() {
            bail!("TUI did not signal readiness for {tab}");
        }
    }
    Ok(0)
}

fn repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("resolving current directory")?;
    loop {
        if dir.join("Cargo.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("unable to locate repo root");
        }
    }
}

fn git_repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("resolving git repository root")?;

    if !output.status.success() {
        bail!(
            "failed to resolve git repository root: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn git(repo_root: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout={}\nstderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn configure_git_hooks_sets_repo_local_hooks_path() {
        let repo = tempdir().expect("temp repo");
        git(repo.path(), &["init"]);

        let hooks_dir = repo.path().join("ops/git-hooks");
        fs::create_dir_all(&hooks_dir).expect("create hooks dir");
        let source_hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("ops/git-hooks/pre-push");
        let target_hook = hooks_dir.join("pre-push");
        fs::copy(&source_hook, &target_hook).expect("copy hook");
        let mut perms = fs::metadata(&target_hook)
            .expect("hook metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_hook, perms).expect("set hook perms");

        configure_git_hooks(repo.path()).expect("configure hooks");

        let output = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["config", "--local", "--get", "core.hooksPath"])
            .output()
            .expect("git config read");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "ops/git-hooks"
        );
        assert!(repo.path().join("ops/git-hooks/pre-push").is_file());
    }
}
