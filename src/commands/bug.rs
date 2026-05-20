use anyhow::{Context, Result, bail};
use std::io::Read;

use crate::cli::{BugAttemptCommands, BugCommands, BugProjectCommands};
use jeryu::bugtracker::{
    AttemptStatus, BugAttemptInput, BugPriority, BugProjectInput, BugSeverity, BugSort, BugStatus,
    CanonicalBugReport, branch_name, parse_report_json,
};
use jeryu::db::bugtracker_repo::BugTrackerRepo;
use jeryu::state;

pub(crate) async fn execute_bug_commands(command: BugCommands) -> Result<i32> {
    let db = state::Db::open().await?;
    let repo = BugTrackerRepo::new(db.pool());
    match command {
        BugCommands::Project(command) => execute_project_command(&repo, command).await?,
        BugCommands::Submit {
            target,
            source,
            json,
            file,
            publish,
            idempotency_key,
        } => {
            if publish {
                bail!("bug publish is not implemented yet; use `jeryu bug sync --dry-run`");
            }
            let mut report = read_report(file, json)?;
            if let Some(target) = target {
                report.target_project = target;
            }
            if source != "auto" {
                report.source_project = source;
            }
            let bug = repo
                .submit_bug(&report, idempotency_key.as_deref(), "cli")
                .await?;
            if json {
                print_json(&bug)?;
            } else {
                println!(
                    "{} {} {} {}",
                    bug.id,
                    bug.status.as_str(),
                    bug.severity.label(),
                    bug.title
                );
                println!("branch: {}", branch_name(&bug.id, &bug.title));
            }
        }
        BugCommands::List {
            project,
            status,
            sort,
            json,
        } => {
            let status = status.as_deref().map(BugStatus::parse).transpose()?;
            let sort = BugSort::parse(&sort)?;
            let project = if project == "all" {
                None
            } else {
                Some(project.as_str())
            };
            let bugs = repo.list_bugs(project, status, sort).await?;
            if json {
                print_json(&bugs)?;
            } else {
                for bug in bugs {
                    println!(
                        "{} {:<13} {} {} d{} attempts:{} {}",
                        bug.id,
                        bug.status.as_str(),
                        bug.severity.label(),
                        bug.priority.label(),
                        bug.difficulty,
                        bug.attempt_count,
                        bug.title
                    );
                }
            }
        }
        BugCommands::Show {
            bug_id,
            history,
            json,
        } => {
            let detail = repo.show_bug(&bug_id).await?;
            if json {
                print_json(&detail)?;
            } else {
                println!(
                    "{} {} {} {}",
                    detail.bug.id,
                    detail.bug.status.as_str(),
                    detail.bug.severity.label(),
                    detail.bug.title
                );
                println!(
                    "{} -> {} component:{}",
                    detail.bug.source_project,
                    detail.bug.target_project,
                    detail.bug.component.as_deref().unwrap_or("-")
                );
                println!(
                    "\n{}",
                    jeryu::bugtracker::render::canonical_markdown(&detail.bug.body)
                );
                if history {
                    println!("Events:");
                    for event in detail.events {
                        println!("{} {} {}", event.created_at, event.event_type, event.actor);
                    }
                    println!("Attempts:");
                    for attempt in detail.attempts {
                        println!(
                            "#{} {} {}",
                            attempt.id,
                            attempt.status.as_str(),
                            attempt.agent.as_deref().unwrap_or("-")
                        );
                    }
                }
            }
        }
        BugCommands::Triage {
            bug_id,
            status,
            severity,
            priority,
            component,
            owner,
        } => {
            let status = status.as_deref().map(BugStatus::parse).transpose()?;
            let severity = severity.as_deref().map(parse_severity).transpose()?;
            let priority = priority.as_deref().map(parse_priority).transpose()?;
            let bug = repo
                .update_bug(
                    &bug_id,
                    status,
                    severity,
                    priority,
                    component.as_deref(),
                    owner.as_deref(),
                    "cli",
                )
                .await?;
            print_json(&bug)?;
        }
        BugCommands::Link {
            bug_id,
            other_id,
            kind,
        } => {
            repo.link_bugs(&bug_id, &other_id, &kind, "cli").await?;
            println!("linked {bug_id} {kind} {other_id}");
        }
        BugCommands::Ready { project, json } => {
            let project = if project == "all" {
                None
            } else {
                Some(project.as_str())
            };
            let bugs = repo.ready_bugs(project).await?;
            if json {
                print_json(&bugs)?;
            } else {
                for bug in bugs {
                    println!("{} {} {}", bug.id, bug.priority.label(), bug.title);
                }
            }
        }
        BugCommands::Attempt(command) => execute_attempt_command(&repo, command).await?,
        BugCommands::Sync {
            bug_id,
            project,
            provider,
            dry_run,
        } => {
            if !dry_run {
                bail!("provider sync currently supports dry-run payload preview only");
            }
            print_json(&serde_json::json!({
                "provider": provider,
                "bug_id": bug_id,
                "project": project,
                "dry_run": true
            }))?;
        }
    }
    Ok(0)
}

async fn execute_project_command(repo: &BugTrackerRepo, command: BugProjectCommands) -> Result<()> {
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

async fn execute_attempt_command(repo: &BugTrackerRepo, command: BugAttemptCommands) -> Result<()> {
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

fn read_report(file: Option<std::path::PathBuf>, json_flag: bool) -> Result<CanonicalBugReport> {
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

fn parse_severity(input: &str) -> Result<BugSeverity> {
    match input {
        "S0" | "s0" => Ok(BugSeverity::S0),
        "S1" | "s1" => Ok(BugSeverity::S1),
        "S2" | "s2" => Ok(BugSeverity::S2),
        "S3" | "s3" => Ok(BugSeverity::S3),
        "S4" | "s4" => Ok(BugSeverity::S4),
        other => bail!("unknown severity '{other}'"),
    }
}

fn parse_priority(input: &str) -> Result<BugPriority> {
    match input {
        "P0" | "p0" => Ok(BugPriority::P0),
        "P1" | "p1" => Ok(BugPriority::P1),
        "P2" | "p2" => Ok(BugPriority::P2),
        "P3" | "p3" => Ok(BugPriority::P3),
        "P4" | "p4" => Ok(BugPriority::P4),
        other => bail!("unknown priority '{other}'"),
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
