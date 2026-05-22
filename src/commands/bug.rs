use anyhow::{Result, bail};

use crate::cli::BugCommands;
use jeryu::bugtracker::{BugSort, BugStatus, branch_name};
use jeryu::bugtracker_records::BugTrackerRepo;

#[path = "bug_support.rs"]
mod bug_support;
use bug_support::*;

#[derive(Debug, thiserror::Error)]
pub(crate) enum BugCommandError {
    #[error("unsupported bug operation `{operation}`: {reason}; use `{remediation}`")]
    UnsupportedOperation {
        operation: &'static str,
        reason: &'static str,
        remediation: &'static str,
    },
}

pub(crate) async fn execute_bug_commands(command: BugCommands) -> Result<i32> {
    match command {
        BugCommands::Project(command) => {
            let repo = bug_repo().await?;
            execute_project_command(&repo, command).await?
        }
        BugCommands::Submit {
            target,
            source,
            json,
            file,
            publish,
            idempotency_key,
        } => {
            if publish {
                return Err(unsupported_bug_publish().into());
            }
            let repo = bug_repo().await?;
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
            let repo = bug_repo().await?;
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
            let repo = bug_repo().await?;
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
            let repo = bug_repo().await?;
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
            let repo = bug_repo().await?;
            repo.link_bugs(&bug_id, &other_id, &kind, "cli").await?;
            println!("linked {bug_id} {kind} {other_id}");
        }
        BugCommands::Ready { project, json } => {
            let repo = bug_repo().await?;
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
        BugCommands::Attempt(command) => {
            let repo = bug_repo().await?;
            execute_attempt_command(&repo, command).await?
        }
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

async fn bug_repo() -> Result<BugTrackerRepo> {
    BugTrackerRepo::open_default().await
}
