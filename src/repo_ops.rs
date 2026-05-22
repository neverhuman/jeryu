use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub async fn jankurai_fast(changed_from: &str) -> Result<i32> {
    let repo_root = git_repo_root()?;
    fs::create_dir_all(repo_root.join("target/jankurai"))?;
    let status = std::process::Command::new("jankurai")
        .current_dir(&repo_root)
        .args([
            "audit",
            ".",
            "--changed-fast",
            "--changed-from",
            changed_from,
            "--mode",
            "advisory",
            "--json",
            "target/jankurai/changed-fast.json",
            "--md",
            "target/jankurai/changed-fast.md",
        ])
        .status()
        .context("running jankurai changed-fast audit")?;
    Ok(status.code().unwrap_or(1))
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

fn redlinedb_bin_path() -> PathBuf {
    match std::env::var_os("REDLINEDB_BIN") {
        Some(path) => PathBuf::from(path),
        None => default_redlinedb_bin_path(),
    }
}

fn default_redlinedb_bin_path() -> PathBuf {
    let home = match std::env::var_os("HOME") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("/home/ubuntu"),
    };
    home.join(".local/bin/redlinedb")
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

pub(crate) fn git_repo_root() -> Result<PathBuf> {
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
