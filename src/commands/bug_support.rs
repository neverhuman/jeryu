use anyhow::{Context, Result, bail};
use std::io::Read;

use crate::cli::{BugAttemptCommands, BugProjectCommands};
use jeryu::bugtracker::{
    AttemptStatus, BugAttemptInput, BugPriority, BugProjectInput, BugSeverity, CanonicalBugReport,
    parse_report_json,
};
use jeryu::bugtracker_records::BugTrackerRepo;

use super::BugCommandError;

pub(crate) fn unsupported_bug_publish() -> BugCommandError {
    BugCommandError::UnsupportedOperation {
        operation: "bug publish",
        reason: "provider publishing is not wired to a supported implementation",
        remediation: "jeryu bug sync --dry-run",
    }
}

pub(crate) async fn execute_project_command(
    repo: &BugTrackerRepo,
    command: BugProjectCommands,
) -> Result<()> {
    match command {
        BugProjectCommands::Add {
            alias,
            repo_root,
            repo_slug,
            provider,
            provider_project_id,
            default_branch,
            json,
        } => {
            let project = repo
                .add_project(&BugProjectInput {
                    alias,
                    repo_root: repo_root.display().to_string(),
                    repo_slug,
                    provider_kind: provider,
                    provider_project_id,
                    default_branch,
                })
                .await?;
            if json {
                print_json(&project)?;
            } else {
                println!("registered {} -> {}", project.alias, project.repo_slug);
            }
        }
        BugProjectCommands::List { json } => {
            let projects = repo.list_projects().await?;
            if json {
                print_json(&projects)?;
            } else {
                for project in projects {
                    println!(
                        "{} {} {}",
                        project.alias, project.provider_kind, project.repo_slug
                    );
                }
            }
        }
        BugProjectCommands::Show { alias, json } => {
            let project = repo.project(&alias).await?;
            if json {
                print_json(&project)?;
            } else {
                println!(
                    "{} {} {}",
                    project.alias, project.provider_kind, project.repo_slug
                );
                println!("root: {}", project.repo_root);
                println!("default branch: {}", project.default_branch);
            }
        }
        BugProjectCommands::Link {
            source,
            target,
            kind,
        } => {
            repo.link_projects(&source, &target, &kind).await?;
            println!("linked project {source} {kind} {target}");
        }
    }
    Ok(())
}

pub(crate) async fn execute_attempt_command(
    repo: &BugTrackerRepo,
    command: BugAttemptCommands,
) -> Result<()> {
    let (bug_id, input) = match command {
        BugAttemptCommands::Start {
            bug_id,
            agent,
            branch,
            sandbox_path,
        } => (
            bug_id,
            BugAttemptInput {
                agent,
                status: AttemptStatus::Started,
                sandbox_path: sandbox_path.map(|p| p.display().to_string()),
                branch,
                base_sha: None,
                head_sha: None,
                pr_url: None,
                ci_evidence: None,
                notes: None,
            },
        ),
        BugAttemptCommands::Fail {
            bug_id,
            agent,
            notes,
            ci_evidence,
        } => (
            bug_id,
            BugAttemptInput {
                agent,
                status: AttemptStatus::Failed,
                sandbox_path: None,
                branch: None,
                base_sha: None,
                head_sha: None,
                pr_url: None,
                ci_evidence,
                notes,
            },
        ),
        BugAttemptCommands::Complete {
            bug_id,
            agent,
            pr_url,
            head_sha,
            notes,
        } => (
            bug_id,
            BugAttemptInput {
                agent,
                status: AttemptStatus::FixProposed,
                sandbox_path: None,
                branch: None,
                base_sha: None,
                head_sha,
                pr_url,
                ci_evidence: None,
                notes,
            },
        ),
    };
    let attempt = repo.record_attempt(&bug_id, &input, "cli").await?;
    print_json(&attempt)?;
    Ok(())
}

pub(crate) fn read_report(
    file: Option<std::path::PathBuf>,
    json_flag: bool,
) -> Result<CanonicalBugReport> {
    let input = if let Some(path) = file {
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?
    } else if json_flag {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("read bug JSON from stdin")?;
        buf
    } else {
        bail!("provide --file <report.json> or --json with report JSON on stdin");
    };
    parse_report_json(&input)
}

pub(crate) fn parse_severity(input: &str) -> Result<BugSeverity> {
    match input {
        "S0" | "s0" => Ok(BugSeverity::S0),
        "S1" | "s1" => Ok(BugSeverity::S1),
        "S2" | "s2" => Ok(BugSeverity::S2),
        "S3" | "s3" => Ok(BugSeverity::S3),
        "S4" | "s4" => Ok(BugSeverity::S4),
        other => bail!("unknown severity '{other}'"),
    }
}

pub(crate) fn parse_priority(input: &str) -> Result<BugPriority> {
    match input {
        "P0" | "p0" => Ok(BugPriority::P0),
        "P1" | "p1" => Ok(BugPriority::P1),
        "P2" | "p2" => Ok(BugPriority::P2),
        "P3" | "p3" => Ok(BugPriority::P3),
        "P4" | "p4" => Ok(BugPriority::P4),
        other => bail!("unknown priority '{other}'"),
    }
}

pub(crate) fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bug_publish_error_shape_is_typed() {
        let err = unsupported_bug_publish();
        match err {
            BugCommandError::UnsupportedOperation {
                operation,
                reason,
                remediation,
            } => {
                assert_eq!(operation, "bug publish");
                assert!(reason.contains("provider publishing"));
                assert_eq!(remediation, "jeryu bug sync --dry-run");
            }
        }
    }
}
