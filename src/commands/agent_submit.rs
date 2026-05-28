//! Owner: Agent submission (host-aware GitHub/GitLab path)
//! Proof: `cargo test -p jeryu -- commands::agent_submit`
//! Invariants: local GitLab uses the native GitLab client; GitHub keeps the existing gh path; no glab fallback.
//!
//! Implements `jeryu agent submit`. Produces an Evidence Capsule and opens a
//! draft GitHub PR via the `gh` CLI or a draft local GitLab MR via the native
//! GitLab client. The capsule is written to
//! `ops/releases/draft/<branch>/capsule.json` so the reviewer-agent and
//! `jeryu release ready` can pick it up.

use anyhow::{Context, Result};
use jeryu::access::{access_findings_for_repo, load_contract, repo_entry_for_path};
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
    let branch = jeryu::access::git_branch_current(&std::env::current_dir()?)?;
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

    if is_local_gitlab_repo()? {
        open_local_gitlab_mr(&capsule, &task, &branch, json).await?;
    } else {
        open_github_draft_pr(&capsule, &task, json)?;
    }
    Ok(())
}

fn is_local_gitlab_repo() -> Result<bool> {
    let cwd = std::env::current_dir()?;
    if jeryu::access::repo_origin_is_local_http(&cwd)? {
        anyhow::bail!(
            "local GitLab HTTP origins are forbidden; run `jeryu access repair --repo . --yes`"
        );
    }
    jeryu::access::repo_is_local_gitlab(&cwd)
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

async fn open_local_gitlab_mr(
    capsule: &EvidenceCapsule,
    task: &str,
    branch: &str,
    json: bool,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let contract = load_contract()?;
    let access_report = access_findings_for_repo(&contract, &cwd)?;
    if access_report
        .findings
        .iter()
        .any(|finding| finding.severity == "error")
    {
        anyhow::bail!("access report has errors; run `jeryu access repair --repo . --yes`");
    }
    let project_path = access_report
        .project_path
        .or_else(|| {
            repo_entry_for_path(&contract, &cwd)
                .map(|entry| format!("{}/{}", entry.namespace, entry.name))
        })
        .ok_or_else(|| anyhow::anyhow!("could not determine local GitLab project path"))?;
    let auth = jeryu::gitlab_auth::resolve_or_repair_default().await?;
    let client = jeryu::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
    let project = client.get_project_by_path(&project_path).await?;
    jeryu::access::git_push_branch(&cwd, "origin", branch)?;
    let title = format!(
        "Draft: [{tier}] {task}",
        tier = tier_label(capsule.risk_tier)
    );
    let mr = client
        .create_merge_request(
            project.id,
            branch,
            &repo_entry_for_path(&contract, &cwd)
                .map(|entry| entry.default_branch.clone())
                .unwrap_or_else(|| "main".to_string()),
            &title,
            &capsule.render_pr_body(),
        )
        .await?;
    if !json {
        println!(
            "📦 Evidence capsule written to {}",
            draft_dir(branch).join("capsule.json").display()
        );
        println!("   Local GitLab project: {project_path}");
        println!("   Branch: {branch}");
        println!("   MR: !{} {}", mr.iid, mr.web_url);
        println!("✓ Draft MR opened.");
    }
    Ok(())
}

fn open_github_draft_pr(capsule: &EvidenceCapsule, task: &str, json: bool) -> Result<()> {
    let branch = jeryu::access::git_branch_current(&std::env::current_dir()?)?;
    let body = capsule.render_pr_body();
    let body_path = draft_dir(&branch).join("pr-body.md");
    std::fs::write(&body_path, &body).with_context(|| format!("write {}", body_path.display()))?;

    let title = format!("[{tier}] {task}", tier = tier_label(capsule.risk_tier));
    open_draft_pr(&title, &body_path).context("open draft PR via gh")?;

    if !json {
        println!("✓ Draft PR opened.");
    }
    Ok(())
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
