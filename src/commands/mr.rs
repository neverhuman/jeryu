//! Owner: Local GitLab merge-request commands.
//! Proof: `cargo test -p jeryu -- commands::mr`
//! Invariants: MR creation stays on the local GitLab client, never glab, and draft state is expressed explicitly.

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::cli::MrCommands;
use jeryu::access::{access_findings_for_repo, load_contract, repo_entry_for_path};

#[derive(Debug, Serialize)]
struct MrCreateReport {
    contract_path: String,
    repo_path: String,
    project_path: String,
    source_branch: String,
    target_branch: String,
    title: String,
    draft: bool,
    push: bool,
    mr_iid: Option<i64>,
    web_url: Option<String>,
    findings: Vec<jeryu::access::AccessFinding>,
}

pub(crate) async fn execute_mr_commands(cmd: MrCommands) -> Result<i32> {
    match cmd {
        MrCommands::Create {
            source,
            target,
            title,
            draft,
            push,
            json,
        } => {
            let report = create_merge_request(&source, &target, &title, draft, push).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_report(&report);
            }
            Ok(
                if report
                    .findings
                    .iter()
                    .any(|finding| finding.severity == "error")
                {
                    1
                } else {
                    0
                },
            )
        }
    }
}

async fn create_merge_request(
    source: &str,
    target: &str,
    title: &str,
    draft: bool,
    push: bool,
) -> Result<MrCreateReport> {
    let repo_path = std::env::current_dir().context("current dir")?;
    let auth = jeryu::gitlab_auth::resolve_or_repair_default().await?;
    let client = jeryu::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
    let contract = load_contract()?;
    let access_report = access_findings_for_repo(&contract, &repo_path)?;
    if access_report
        .findings
        .iter()
        .any(|finding| finding.severity == "error")
    {
        bail!("access report has errors; run `jeryu access repair --repo . --yes`");
    }
    if access_report.origin_url.is_none() {
        bail!("cannot create merge request without an origin remote");
    }
    if jeryu::access::repo_origin_is_local_http(&repo_path)? {
        bail!("local GitLab HTTP origins are forbidden; run `jeryu access repair --repo . --yes`");
    }
    if !jeryu::access::repo_is_local_gitlab(&repo_path)? {
        bail!("`jeryu mr create` only supports local GitLab SSH checkouts");
    }

    let project_path = access_report
        .project_path
        .clone()
        .or_else(|| {
            repo_entry_for_path(&contract, &repo_path)
                .map(|entry| format!("{}/{}", entry.namespace, entry.name))
        })
        .ok_or_else(|| anyhow::anyhow!("could not determine project path"))?;
    let project = client.get_project_by_path(&project_path).await?;

    if push {
        jeryu::access::git_push_branch(&repo_path, "origin", source)?;
    }

    let final_title = if draft {
        if title.starts_with("Draft:")
            || title.starts_with("[Draft]")
            || title.starts_with("(Draft)")
        {
            title.to_string()
        } else {
            format!("Draft: {title}")
        }
    } else {
        title.to_string()
    };

    let mr = client
        .create_merge_request(
            project.id,
            source,
            target,
            &final_title,
            &format!(
                "Created by jeryu on {}\n\nProject: {}\n",
                repo_path.display(),
                project_path
            ),
        )
        .await?;

    Ok(MrCreateReport {
        contract_path: jeryu::access::contract_path().display().to_string(),
        repo_path: repo_path.display().to_string(),
        project_path,
        source_branch: source.to_string(),
        target_branch: target.to_string(),
        title: final_title,
        draft,
        push,
        mr_iid: Some(mr.iid),
        web_url: Some(mr.web_url),
        findings: access_report.findings,
    })
}

fn print_report(report: &MrCreateReport) {
    println!("merge request created for {}", report.project_path);
    println!("  source: {}", report.source_branch);
    println!("  target: {}", report.target_branch);
    println!("  title:  {}", report.title);
    if let Some(iid) = report.mr_iid {
        println!("  iid:    !{iid}");
    }
    if let Some(url) = &report.web_url {
        println!("  web:    {url}");
    }
    for finding in &report.findings {
        println!(
            "  [{}] {}: {}",
            finding.severity, finding.code, finding.message
        );
    }
}
