//! Owner: Access contract and local GitLab repair commands.
//! Proof: `cargo test -p jeryu -- commands::access`
//! Invariants: local GitLab access reports are secret-free, fail closed on HTTP origins, and never invoke glab.

use anyhow::{Context, Result, bail};

use crate::cli::{AccessCommands, AccessKeyCommands};
use jeryu::access::{
    AccessDoctorReport, AccessKeysReport, AccessProjectReport, AccessRepairReport,
    access_findings_for_repo, build_keys_report, cleanup_local_http_gitlab_credentials,
    ensure_contract_file, load_contract, repair_repo_layout, resolve_project_report,
};
use std::path::{Path, PathBuf};

pub(crate) async fn execute_access_commands(cmd: AccessCommands) -> Result<i32> {
    match cmd {
        AccessCommands::Doctor {
            repo,
            all_known,
            json,
        } => {
            let reports = doctor_reports(repo, all_known).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&reports)?);
            } else {
                print_doctor_reports(&reports);
            }
            Ok(if reports.iter().all(|r| r.findings.is_empty()) {
                0
            } else {
                1
            })
        }
        AccessCommands::Repair {
            repo,
            all_known,
            yes,
            json,
        } => {
            if !yes {
                bail!("refusing to repair without --yes");
            }
            let reports = repair_reports(repo, all_known).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&reports)?);
            } else {
                print_repair_reports(&reports);
            }
            Ok(0)
        }
        AccessCommands::Project {
            project,
            repo,
            json,
        } => {
            let report = project_report(project.as_deref(), repo.as_deref()).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_project_report(&report);
            }
            Ok(if report.findings.is_empty() { 0 } else { 1 })
        }
        AccessCommands::Keys(subcmd) => match subcmd {
            AccessKeyCommands::List { json } => {
                let report = keys_report().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_keys_report(&report);
                }
                Ok(if report.findings.is_empty() { 0 } else { 1 })
            }
            AccessKeyCommands::Doctor { json } => {
                let report = keys_report().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    print_keys_report(&report);
                }
                Ok(if report.findings.is_empty() { 0 } else { 1 })
            }
        },
    }
}

async fn doctor_reports(repo: Option<PathBuf>, all_known: bool) -> Result<Vec<AccessDoctorReport>> {
    let contract = load_contract()?;
    let repo_paths = target_paths(&contract, repo, all_known)?;
    let mut reports = Vec::new();
    let token = jeryu::gitlab_auth::load_token_for_url(&contract.local_api_url)?;
    let client = token
        .map(|token| jeryu::gitlab_client::GitlabClient::new(&contract.local_api_url, Some(token)));

    for repo_path in repo_paths {
        let mut report = access_findings_for_repo(&contract, &repo_path)?;
        if let Some(client) = &client
            && let Some(project_path) = report.project_path.clone()
        {
            match resolve_project_report(&contract, &repo_path, Some(&project_path), client).await {
                Ok(project) => {
                    report.project_id = project.project_id;
                    report.web_url = project.web_url;
                    report.findings.extend(project.findings);
                }
                Err(err) => {
                    report.findings.push(jeryu::access::AccessFinding {
                        code: "project_resolution_error".to_string(),
                        severity: "error".to_string(),
                        message: err.to_string(),
                        repair_hint: Some("jeryu access repair --repo . --yes".to_string()),
                    });
                }
            }
        } else if report.project_path.is_some() {
            report.findings.push(jeryu::access::AccessFinding {
                code: "missing_gitlab_pat".to_string(),
                severity: "error".to_string(),
                message: "local GitLab PAT is missing; cannot resolve project".to_string(),
                repair_hint: Some("jeryu bootstrap".to_string()),
            });
        }
        reports.push(report);
    }

    Ok(reports)
}

async fn repair_reports(repo: Option<PathBuf>, all_known: bool) -> Result<Vec<AccessRepairReport>> {
    let contract = ensure_contract_file()?;
    let repo_paths = target_paths(&contract, repo, all_known)?;
    let mut reports = Vec::new();
    let auth = jeryu::gitlab_auth::resolve_or_repair_default().await?;
    let client = jeryu::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));

    for repo_path in repo_paths {
        let mut report = repair_repo_layout(&contract, &repo_path)?;
        if let Some(project_path) = report.repo_slug.clone()
            && let Err(err) =
                resolve_project_report(&contract, &repo_path, Some(&project_path), &client).await
        {
            report.findings.push(jeryu::access::AccessFinding {
                code: "project_resolution_error".to_string(),
                severity: "warning".to_string(),
                message: err.to_string(),
                repair_hint: Some("retry after local GitLab recovers".to_string()),
            });
        }
        reports.push(report);
    }

    if cleanup_local_http_gitlab_credentials().unwrap_or(false) {
        for report in &mut reports {
            report.credential_cleanup = true;
        }
    }

    Ok(reports)
}

async fn project_report(project: Option<&str>, repo: Option<&Path>) -> Result<AccessProjectReport> {
    let contract = load_contract()?;
    let auth = jeryu::gitlab_auth::resolve_or_repair_default().await?;
    let client = jeryu::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
    let repo_path = repo
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_dir().context("current dir")?);
    resolve_project_report(&contract, &repo_path, project, &client).await
}

async fn keys_report() -> Result<AccessKeysReport> {
    let contract = load_contract()?;
    Ok(build_keys_report(&contract))
}

fn target_paths(
    contract: &jeryu::access::AccessContract,
    repo: Option<PathBuf>,
    all_known: bool,
) -> Result<Vec<PathBuf>> {
    if all_known && repo.is_some() {
        bail!("use either --repo or --all-known, not both");
    }
    if all_known {
        return Ok(contract
            .known_repos
            .iter()
            .map(|entry| PathBuf::from(&entry.path))
            .collect());
    }
    Ok(vec![match repo {
        Some(path) => path,
        None => std::env::current_dir().context("current dir")?,
    }])
}

fn print_doctor_reports(reports: &[AccessDoctorReport]) {
    for report in reports {
        println!("access doctor: {}", report.repo_path);
        if let Some(origin) = &report.origin_url {
            println!("  origin: {origin}");
        } else {
            println!("  origin: (missing)");
        }
        if let Some(expected) = &report.expected_origin_url {
            println!("  expected: {expected}");
        }
        for finding in &report.findings {
            println!(
                "  [{}] {}: {}",
                finding.severity, finding.code, finding.message
            );
            if let Some(hint) = &finding.repair_hint {
                println!("    hint: {hint}");
            }
        }
    }
}

fn print_repair_reports(reports: &[AccessRepairReport]) {
    for report in reports {
        println!("access repair: {}", report.repo_path);
        if let Some(after) = &report.origin_after {
            println!("  origin: {after}");
        }
        for written in &report.written_files {
            println!("  wrote: {written}");
        }
        if report.credential_cleanup {
            println!("  cleaned: ~/.git-credentials");
        }
        for finding in &report.findings {
            println!(
                "  [{}] {}: {}",
                finding.severity, finding.code, finding.message
            );
        }
    }
}

fn print_project_report(report: &AccessProjectReport) {
    println!("access project: {}", report.project_path);
    println!("  repo: {}", report.repo_path);
    if let Some(id) = report.project_id {
        println!("  project_id: {id}");
    }
    if let Some(web_url) = &report.web_url {
        println!("  web_url: {web_url}");
    }
    for finding in &report.findings {
        println!(
            "  [{}] {}: {}",
            finding.severity, finding.code, finding.message
        );
    }
}

fn print_keys_report(report: &AccessKeysReport) {
    println!("access keys:");
    for key in &report.keys {
        println!(
            "  {} | {} | {} | exists={} readable={}",
            key.alias, key.fingerprint, key.identity_path, key.exists, key.readable
        );
    }
    for finding in &report.findings {
        println!(
            "  [{}] {}: {}",
            finding.severity, finding.code, finding.message
        );
    }
}
