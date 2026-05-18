//! Owner: Agent submission (GitHub PR path)
//! Proof: `cargo test -p jeryu -- commands::agent_submit`
//! Invariants: GitHub-only. Never requires GITLAB_PAT.
//!
//! Implements `jeryu agent submit`. Produces an Evidence Capsule and opens a
//! draft GitHub PR via the `gh` CLI. The capsule is written to
//! `ops/releases/draft/<branch>/capsule.json` so the reviewer-agent and
//! `jeryu release ready` can pick it up.

use anyhow::{Context, Result};
use jeryu::release::EvidenceCapsule;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) async fn execute_agent_submit(
    task: String,
    issue: Option<u64>,
    risk_tier: Option<u8>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let branch = current_branch()?;
    let agent_id = match std::env::var("JERYU_AGENT_ID") {
        Ok(id) => id,
        Err(_) => "human:local".to_string(),
    };

    let mut capsule = EvidenceCapsule::new(&agent_id, &task, &branch);
    capsule.issue = issue;
    if let Some(t) = risk_tier {
        capsule.risk_tier = t;
    }

    let evidence_dir = draft_dir(&branch);
    let capsule_path = capsule
        .write(evidence_dir.clone())
        .context("write evidence capsule")?;

    if json {
        println!("{}", serde_json::to_string_pretty(&capsule)?);
    } else {
        println!("📦 Evidence capsule written to {}", capsule_path.display());
        println!("   Branch: {branch}");
        println!("   Agent:  {agent_id}");
        if let Some(i) = issue {
            println!("   Issue:  #{i}");
        }
        println!("   Tier:   {}", capsule.risk_tier);
    }

    if dry_run {
        if !json {
            println!(
                "--dry-run: PR not opened. Capsule is at {}",
                capsule_path.display()
            );
        }
        return Ok(());
    }

    let body = capsule.render_pr_body();
    let body_path = evidence_dir.join("pr-body.md");
    std::fs::write(&body_path, &body).with_context(|| format!("write {}", body_path.display()))?;

    let title = format!("[{tier}] {task}", tier = tier_label(capsule.risk_tier));
    open_draft_pr(&title, &body_path).context("open draft PR via gh")?;

    if !json {
        println!("✓ Draft PR opened.");
    }
    Ok(())
}

fn current_branch() -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("run git rev-parse")?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(anyhow::anyhow!("could not determine current branch"));
    }
    Ok(s)
}

fn draft_dir(branch: &str) -> PathBuf {
    let safe = branch.replace('/', "_");
    PathBuf::from("ops/releases/draft").join(safe)
}

fn tier_label(tier: u8) -> &'static str {
    match tier {
        0 => "docs",
        1 => "bugfix",
        2 => "feature",
        3 => "release",
        4 => "emergency",
        _ => "tier-x",
    }
}

fn open_draft_pr(title: &str, body_path: &Path) -> Result<()> {
    let out = Command::new("gh")
        .args([
            "pr",
            "create",
            "--draft",
            "--title",
            title,
            "--body-file",
            &body_path.to_string_lossy(),
        ])
        .output()
        .context("invoke gh pr create (is `gh` installed and authenticated?)")?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "gh pr create failed (exit={:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_dir_replaces_slashes() {
        let p = draft_dir("agent/claude/1-fix-x");
        assert_eq!(p, PathBuf::from("ops/releases/draft/agent_claude_1-fix-x"));
    }

    #[test]
    fn tier_label_known() {
        assert_eq!(tier_label(0), "docs");
        assert_eq!(tier_label(3), "release");
        assert_eq!(tier_label(99), "tier-x");
    }
}
