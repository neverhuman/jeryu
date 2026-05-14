use anyhow::Result;
use jeryu::release;

#[derive(Debug, serde::Serialize)]
pub(super) struct ReleaseDryRunReport {
    pub version: String,
    pub checks: Vec<(String, String)>,
    pub blockers: Vec<String>,
}

pub(super) async fn run_release_dry_run(version: &str) -> ReleaseDryRunReport {
    let mut checks: Vec<(String, String)> = Vec::new();
    let mut blockers: Vec<String> = Vec::new();

    // Version consistency: VERSION, version.json, Cargo.toml workspace package.version.
    match std::fs::read_to_string("VERSION") {
        Ok(s) => {
            let trimmed = s.trim().to_string();
            checks.push(("VERSION".into(), trimmed.clone()));
            if !version.starts_with(&trimmed) && trimmed != *version {
                blockers.push(format!(
                    "VERSION ({trimmed}) does not match requested release version ({version})"
                ));
            }
        }
        Err(e) => blockers.push(format!("read VERSION: {e}")),
    }
    if !std::path::Path::new("CHANGELOG.md").exists() {
        blockers.push("CHANGELOG.md missing".into());
    } else {
        checks.push(("CHANGELOG.md".into(), "present".into()));
    }
    if !std::path::Path::new("release.policy.toml").exists() {
        blockers.push("release.policy.toml missing".into());
    } else {
        checks.push(("release.policy.toml".into(), "present".into()));
    }

    let preflight = release::release_preflight(None).await;
    checks.push((
        "preflight".into(),
        if preflight.ok { "PASS".into() } else { "FAIL".into() },
    ));
    if !preflight.ok {
        blockers.push(format!(
            "preflight failed with {} blocker(s)",
            preflight.blockers.len()
        ));
    }

    ReleaseDryRunReport { version: version.to_string(), checks, blockers }
}

pub(super) async fn run_release_submit(version: &str, force: bool, dry_run: bool) -> Result<()> {
    let out = tokio::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .await?;
    if !out.stdout.is_empty() {
        return Err(anyhow::anyhow!(
            "working tree is not clean; commit or stash before `release submit`"
        ));
    }

    if !force {
        let cached_path = format!(".jeryu/release-submit-cache/{version}.ok");
        let fresh = std::fs::metadata(&cached_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| t.elapsed().map(|e| e.as_secs() < 1800).unwrap_or(false))
            .unwrap_or(false);
        if !fresh {
            return Err(anyhow::anyhow!(
                "no fresh `release dry-run` result found at {} \
                 (re-run `jeryu release dry-run --version {}` first, or pass --force)",
                cached_path,
                version
            ));
        }
    }

    if dry_run {
        println!("--dry-run: would tag v{version}, push, and trigger release.yml");
        return Ok(());
    }

    let tag = format!("v{version}");
    run("git", &["tag", "-a", &tag, "-m", &format!("Release {tag}")]).await?;
    run("git", &["push", "origin", &tag]).await?;
    run(
        "gh",
        &[
            "workflow",
            "run",
            "release.yml",
            "-f",
            &format!("version={version}"),
        ],
    )
    .await?;
    println!("Submitted release {tag}. Track via `jeryu release watch`.");
    Ok(())
}

pub(super) async fn run_release_approve(
    pr: u64,
    as_user: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let approver = if let Some(u) = as_user {
        u
    } else {
        let out = tokio::process::Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("gh api user failed: {e}"))?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "gh api user failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let pr_str = pr.to_string();
    let author_out = tokio::process::Command::new("gh")
        .args(["pr", "view", &pr_str, "--json", "author", "--jq", ".author.login"])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("gh pr view failed: {e}"))?;
    if !author_out.status.success() {
        return Err(anyhow::anyhow!(
            "gh pr view {pr} failed: {}",
            String::from_utf8_lossy(&author_out.stderr)
        ));
    }
    let author = String::from_utf8_lossy(&author_out.stdout).trim().to_string();

    if !approver.is_empty() && approver == author {
        return Err(anyhow::anyhow!(
            "self-approval refused: PR author and approver are both `{approver}`"
        ));
    }

    let state_out = tokio::process::Command::new("gh")
        .args(["pr", "view", &pr_str, "--json", "statusCheckRollup"])
        .output()
        .await?;
    if state_out.status.success() {
        let body = String::from_utf8_lossy(&state_out.stdout);
        if body.contains("FAILURE") || body.contains("ERROR") {
            return Err(anyhow::anyhow!(
                "CI is not green for PR {pr}; refusing to approve"
            ));
        }
    }

    if dry_run {
        println!("--dry-run: would approve PR {pr} as `{approver}` (author={author})");
        return Ok(());
    }

    run("gh", &["pr", "review", &pr_str, "--approve"]).await?;
    println!("Approved PR #{pr} as `{approver}`.");
    Ok(())
}

pub(super) fn trim_head(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

pub(super) async fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let out = tokio::process::Command::new(cmd).args(args).output().await?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "{} {} failed (exit={:?}): {}",
            cmd,
            args.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}
