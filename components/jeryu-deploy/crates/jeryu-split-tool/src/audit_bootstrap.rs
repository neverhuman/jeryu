//! Reuse the owning public auditor installation verifier.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(super) fn pin(root: &Path, name: &str) -> Result<String> {
    let source = fs::read_to_string(root.join("components/jeryu-tool/generated/jankurai-pin.env"))?;
    let prefix = format!("{name}=\"");
    let matches: Vec<_> = source
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix).and_then(|v| v.strip_suffix('"')))
        .collect();
    ensure!(
        matches.len() == 1,
        "missing or duplicate auditor pin: {name}"
    );
    Ok(matches[0].to_owned())
}

pub(super) fn verify_auditor(
    root: &Path,
    auditor: &Path,
    receipt: &Path,
    commit: &str,
    log: &Path,
) -> Result<()> {
    // Reuse the owning closed receipt, pin, renderer and installation verifier.
    // A receipt hash by itself supplies no installation or publisher authority.
    let log = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log)?;
    let mut command = Command::new("/usr/bin/timeout");
    let user_home = std::env::var_os("HOME").context("HOME required by auditor verifier")?;
    let cargo_bin = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&user_home).join(".cargo"))
        .join("bin");
    ensure!(cargo_bin.is_absolute(), "Cargo tool path must be absolute");
    command
        .args([
            "--kill-after=10s",
            "180",
            "/bin/bash",
            "-c",
            "source \"$1\"; require_public_candidate_jankurai",
            "audit-verifier",
        ])
        .arg(root.join("components/jeryu-tool/ops/verify-public-candidate.sh"))
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/local/bin:/usr/bin:/bin", cargo_bin.display()),
        )
        .env("HOME", user_home)
        .env("JERYU_MONOREPO_CANDIDATE", "1")
        .env("JERYU_MONOREPO_EXPECTED_HEAD", commit)
        .env("JERYU_GOVERNED_JANKURAI_BIN", auditor)
        .env("JERYU_JANKURAI_RECEIPT", receipt)
        .env("CARGO_BUILD_JOBS", "2")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    for variable in ["CARGO_HOME", "RUSTUP_HOME"] {
        if let Some(value) = std::env::var_os(variable) {
            command.env(variable, value);
        }
    }
    ensure!(
        command.status()?.success(),
        "public auditor receipt verification failed; inspect private auditor-verification.log"
    );
    Ok(())
}
