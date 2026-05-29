use super::*;
use crate::gitlab_client::MergeRequest;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct FullPathOptions {
    pub source: String,
    pub target: String,
    pub version: Option<String>,
    pub push: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FullPathStage {
    pub name: String,
    pub status: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FullPathReport {
    pub generated_at: String,
    pub source: String,
    pub target: String,
    pub version: String,
    pub source_sha: String,
    pub project_path: String,
    pub push: bool,
    pub mr_iid: Option<i64>,
    pub source_pipeline_id: Option<i64>,
    pub main_pipeline_id: Option<i64>,
    pub production_pipeline_id: Option<i64>,
    pub github_handoff: Option<String>,
    pub deploy_artifact: String,
    pub health_url: String,
    pub success: bool,
    pub stages: Vec<FullPathStage>,
}

impl FullPathReport {
    fn new(
        source: String,
        target: String,
        version: String,
        source_sha: String,
        project_path: String,
        push: bool,
    ) -> Self {
        Self {
            generated_at: chrono::Utc::now().to_rfc3339(),
            source,
            target,
            version,
            source_sha,
            project_path,
            push,
            mr_iid: None,
            source_pipeline_id: None,
            main_pipeline_id: None,
            production_pipeline_id: None,
            github_handoff: None,
            deploy_artifact: deploy_artifact_path().display().to_string(),
            health_url: health_probe_url(),
            success: false,
            stages: Vec::new(),
        }
    }
}

pub async fn run_full_path(options: FullPathOptions) -> Result<FullPathReport> {
    let cwd = std::env::current_dir().context("current dir")?;
    let current_branch = crate::access::git_branch_current(&cwd)?;
    if normalize_ref(&current_branch) != normalize_ref(&options.source) {
        bail!(
            "current branch `{}` does not match --source `{}`",
            current_branch,
            options.source
        );
    }

    if crate::access::repo_origin_is_local_http(&cwd)? {
        bail!("local GitLab HTTP origins are forbidden; run `jeryu access repair --repo . --yes`");
    }

    let contract = crate::access::load_contract()?;
    let access_report = crate::access::access_findings_for_repo(&contract, &cwd)?;
    if access_report
        .findings
        .iter()
        .any(|finding| finding.severity == "error")
    {
        bail!("access report has errors; run `jeryu access repair --repo . --yes`");
    }

    let project_path = access_report
        .project_path
        .or_else(|| {
            crate::access::repo_entry_for_path(&contract, &cwd)
                .map(|entry| format!("{}/{}", entry.namespace, entry.name))
        })
        .ok_or_else(|| anyhow::anyhow!("could not determine local GitLab project path"))?;

    let source_sha = git_rev_parse(&cwd, &options.source)?;
    let version = resolve_release_version(options.version.as_deref(), &source_sha)?;
    let mut report = FullPathReport::new(
        options.source.clone(),
        options.target.clone(),
        version.clone(),
        source_sha.clone(),
        project_path.clone(),
        options.push,
    );

    let auth = crate::gitlab_auth::resolve_or_repair_default().await?;
    let client = crate::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
    let db = crate::state::Db::open().await?;
    let project = {
        let mut attempts = 0usize;
        loop {
            attempts += 1;
            match client.get_project_by_path(&project_path).await {
                Ok(project) => break project,
                Err(err) if attempts < 5 => {
                    eprintln!(
                        "retrying GitLab project lookup for {} after error: {}",
                        project_path, err
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(err) => return Err(err).context("lookup GitLab project by path"),
            }
        }
    };

    if options.push {
        crate::access::git_push_branch(&cwd, "origin", &options.source)?;
    }

    push_stage(
        &mut report,
        options.json,
        "access",
        "pass",
        "local GitLab access and source branch preflight passed",
        Some(json!({
            "project_path": project_path,
            "source_sha": source_sha,
            "version": version,
            "push": options.push,
            "current_branch": current_branch,
            "findings": access_report.findings,
        })),
    )?;

    let title = format!(
        "Release {version} ({source} -> {target})",
        source = options.source,
        target = options.target
    );
    let body = render_merge_request_body(&report, &project.web_url);
    let (mr, mr_detail) = ensure_release_merge_request(
        &client,
        project.id,
        &options.source,
        &options.target,
        &title,
        &body,
    )
    .await
    .context("create or update release merge request")?;
    report.mr_iid = Some(mr.iid);
    push_stage(
        &mut report,
        options.json,
        "mr",
        "pass",
        &mr_detail,
        Some(json!({
            "mr_iid": mr.iid,
            "web_url": mr.web_url,
            "title": title,
        })),
    )?;

    let source_pipeline_ref = format!("refs/merge-requests/{}/head", mr.iid);
    let pipeline = wait_for_source_pipeline(
        &client,
        project.id,
        &source_pipeline_ref,
        &source_sha,
        options.json,
        &mut report,
    )
    .await?;
    report.source_pipeline_id = Some(pipeline.id);
    push_stage(
        &mut report,
        options.json,
        "ci",
        "pass",
        "source pipeline reached success",
        Some(json!({
            "pipeline_id": pipeline.id,
            "status": pipeline.status,
            "sha": pipeline.sha,
        })),
    )?;

    let trust_tier = crate::decision::TrustTier::Trusted;
    let evaluation = crate::agent::merge_agent_mr(&client, project.id, mr.iid, trust_tier).await?;
    if evaluation.decision != crate::decision::RiskGateDecision::Allow {
        push_stage(
            &mut report,
            options.json,
            "risk-gate",
            "fail",
            &evaluation.reason,
            Some(json!({
                "decision": evaluation.decision,
                "reason": evaluation.reason,
                "trust_tier": evaluation.trust_tier,
            })),
        )?;
        return Err(anyhow::anyhow!(
            "risk gate denied merge: {}",
            evaluation.reason
        ));
    }

    push_stage(
        &mut report,
        options.json,
        "risk-gate",
        "pass",
        &evaluation.reason,
        Some(json!({
            "decision": evaluation.decision,
            "reason": evaluation.reason,
            "trust_tier": evaluation.trust_tier,
        })),
    )?;

    let merge_result = client
        .get_merge_request(project.id, mr.iid)
        .await
        .context("refresh merge request after risk gate")?;
    push_stage(
        &mut report,
        options.json,
        "merge",
        "pass",
        "merge request accepted by GitLab",
        Some(json!({
            "mr_iid": mr.iid,
            "state": merge_result.state,
            "web_url": merge_result.web_url,
        })),
    )?;

    let github_handoff =
        perform_github_handoff(&mut report, &options, &project_path, &title).await?;
    report.github_handoff = Some(github_handoff.clone());

    let source_sha = report.source_sha.clone();
    let promotion = wait_for_production_promotion(
        &client,
        &db,
        project.id,
        &options.target,
        &source_sha,
        options.json,
        &mut report,
    )
    .await?;
    report.main_pipeline_id = promotion.main_pipeline_id;
    report.production_pipeline_id = promotion.production_pipeline_id;
    let main_pipeline_id = report.main_pipeline_id;
    let production_pipeline_id = report.production_pipeline_id;
    let promotion_report_version = report.version.clone();
    push_stage(
        &mut report,
        options.json,
        "promotion",
        "pass",
        "mainline promotion completed",
        Some(json!({
            "main_pipeline_id": main_pipeline_id,
            "production_pipeline_id": production_pipeline_id,
            "version": promotion_report_version,
        })),
    )?;

    let deploy_artifact = report.deploy_artifact.clone();
    install_from_artifact(&deploy_artifact_path())
        .await
        .context("install release artifact from deploy lane")?;
    push_stage(
        &mut report,
        options.json,
        "deploy",
        "pass",
        "artifact installed through post-merge deploy lane",
        Some(json!({
            "artifact": deploy_artifact,
            "script": deploy_lane_script().display().to_string(),
        })),
    )?;

    let health = probe_health().await.context("probe local web health")?;
    let health_url = report.health_url.clone();
    if !health.ok {
        push_stage(
            &mut report,
            options.json,
            "health",
            "fail",
            &health.detail,
            Some(json!({
                "status": health.status_code,
                "body": health.body,
                "url": health_url,
            })),
        )?;
        return Err(anyhow::anyhow!(
            "local health probe failed: {}",
            health.detail
        ));
    }
    push_stage(
        &mut report,
        options.json,
        "health",
        "pass",
        &health.detail,
        Some(json!({
            "status": health.status_code,
            "body": health.body,
            "url": health_url,
        })),
    )?;

    report.success = true;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(report)
}

pub fn render_full_path_text(report: &FullPathReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(out, "jeryu release full-path");
    let _ = writeln!(out, "  source:   {}", report.source);
    let _ = writeln!(out, "  target:   {}", report.target);
    let _ = writeln!(out, "  version:  {}", report.version);
    let _ = writeln!(out, "  project:  {}", report.project_path);
    let _ = writeln!(out, "  sha:      {}", report.source_sha);
    if let Some(iid) = report.mr_iid {
        let _ = writeln!(out, "  mr:       !{}", iid);
    }
    if let Some(mode) = &report.github_handoff {
        let _ = writeln!(out, "  github:   {}", mode);
    }
    let _ = writeln!(out, "  deploy:   {}", report.deploy_artifact);
    let _ = writeln!(out, "  health:   {}", report.health_url);
    let _ = writeln!(out, "  success:  {}", report.success);
    if !report.stages.is_empty() {
        let _ = writeln!(out, "\nStages:");
        for stage in &report.stages {
            let _ = writeln!(
                out,
                "  [{}] {} - {}",
                stage.status, stage.name, stage.detail
            );
        }
    }
    out
}

fn push_stage(
    report: &mut FullPathReport,
    json: bool,
    name: &str,
    status: &str,
    detail: &str,
    data: Option<serde_json::Value>,
) -> Result<()> {
    let stage = FullPathStage {
        name: name.to_string(),
        status: status.to_string(),
        detail: detail.to_string(),
        data,
    };
    if json {
        println!("{}", serde_json::to_string(&stage)?);
    }
    report.stages.push(stage);
    Ok(())
}

fn normalize_ref(value: &str) -> String {
    if value.starts_with("refs/heads/") {
        value.trim_start_matches("refs/heads/").to_string()
    } else {
        value.to_string()
    }
}

fn git_rev_parse(repo: &Path, reference: &str) -> Result<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo)
        .args(["rev-parse", reference])
        .output()
        .with_context(|| format!("resolve git ref {reference}"))?;
    if !output.status.success() {
        bail!(
            "git rev-parse {reference} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn resolve_release_version(explicit: Option<&str>, source_sha: &str) -> Result<String> {
    if let Some(version) = explicit.filter(|value| !value.trim().is_empty()) {
        return Ok(version.trim().to_string());
    }
    if let Ok(file_version) = fs::read_to_string("VERSION") {
        let trimmed = file_version.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Ok(render_release_version(source_sha))
}

fn render_merge_request_body(report: &FullPathReport, web_url: &str) -> String {
    format!(
        "Automated release handoff for {version}\n\n\
         Source: `{source}`\n\
         Target: `{target}`\n\
         SHA: `{sha}`\n\
         GitLab project: `{project}`\n\
         Web URL: {web_url}\n",
        version = report.version,
        source = report.source,
        target = report.target,
        sha = report.source_sha,
        project = report.project_path,
        web_url = web_url
    )
}

async fn ensure_release_merge_request(
    client: &GitlabClient,
    project_id: i64,
    source: &str,
    target: &str,
    title: &str,
    body: &str,
) -> Result<(MergeRequest, String)> {
    let existing = client
        .list_open_merge_requests_for_branches(project_id, source, target)
        .await
        .context("list existing release merge requests")?;
    if let Some(mr) = existing.into_iter().next() {
        let updated = client
            .update_merge_request_metadata(project_id, mr.iid, title, body)
            .await
            .context("update existing release merge request")?;
        return Ok((updated, "merge request updated".to_string()));
    }

    let mr = client
        .create_merge_request(project_id, source, target, title, body)
        .await
        .context("create release merge request")?;
    Ok((mr, "merge request created".to_string()))
}

async fn wait_for_source_pipeline(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    expected_sha: &str,
    json: bool,
    report: &mut FullPathReport,
) -> Result<Pipeline> {
    let mut attempts = 0usize;
    loop {
        attempts += 1;
        let pipeline = match latest_pipeline_for_ref(client, project_id, ref_name).await {
            Ok(pipeline) => pipeline,
            Err(err) => {
                if json {
                    push_stage(
                        report,
                        json,
                        "ci",
                        "wait",
                        "waiting for source pipeline lookup to recover",
                        Some(json!({
                            "attempt": attempts,
                            "error": err.to_string(),
                            "ref": ref_name,
                        })),
                    )?;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        let Some(pipeline) = pipeline else {
            if json {
                push_stage(
                    report,
                    json,
                    "ci",
                    "wait",
                    "waiting for source pipeline to appear",
                    Some(json!({ "attempt": attempts })),
                )?;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        if pipeline.sha != expected_sha {
            if json {
                push_stage(
                    report,
                    json,
                    "ci",
                    "wait",
                    "waiting for source pipeline to reach expected sha",
                    Some(json!({
                        "pipeline_id": pipeline.id,
                        "latest_sha": pipeline.sha,
                        "expected_sha": expected_sha,
                        "attempt": attempts,
                    })),
                )?;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        match pipeline.status.as_str() {
            "success" => return Ok(pipeline),
            "failed" | "canceled" => {
                return Err(anyhow::anyhow!(
                    "source pipeline {} ended with status {}",
                    pipeline.id,
                    pipeline.status
                ));
            }
            _ => {
                let doctor =
                    match build_pipeline_doctor_report(client, project_id, pipeline.id).await {
                        Ok(doctor) => doctor,
                        Err(err) => {
                            if json {
                                push_stage(
                                    report,
                                    json,
                                    "ci",
                                    "wait",
                                    "waiting for source pipeline doctor lookup to recover",
                                    Some(json!({
                                        "pipeline_id": pipeline.id,
                                        "status": pipeline.status,
                                        "sha": pipeline.sha,
                                        "attempt": attempts,
                                        "error": err.to_string(),
                                    })),
                                )?;
                            }
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                    };
                if let Some(issue) = doctor
                    .stuck_suspected
                    .iter()
                    .find_map(|job| job.runner_eligibility_issue.clone())
                {
                    return Err(anyhow::anyhow!(
                        "runner eligibility blockage detected for pipeline {}: {}",
                        pipeline.id,
                        issue
                    ));
                }
                if json {
                    push_stage(
                        report,
                        json,
                        "ci",
                        "wait",
                        "source pipeline still running",
                        Some(json!({
                            "pipeline_id": pipeline.id,
                            "status": pipeline.status,
                            "sha": pipeline.sha,
                            "stuck_suspected": doctor.stuck_suspected,
                        })),
                    )?;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

struct PromotionState {
    main_pipeline_id: Option<i64>,
    production_pipeline_id: Option<i64>,
}

async fn wait_for_production_promotion(
    client: &GitlabClient,
    db: &crate::state::Db,
    project_id: i64,
    ref_name: &str,
    expected_sha: &str,
    json: bool,
    report: &mut FullPathReport,
) -> Result<PromotionState> {
    let mut attempts = 0usize;
    loop {
        attempts += 1;
        let release_report =
            reconcile_release_for_ref(db, client, project_id, ref_name, false).await?;
        let latest = release_report
            .latest
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no release attempt found for ref {ref_name}"))?;
        let promotion = maybe_trigger_production_promotion(
            db,
            client,
            project_id,
            ref_name,
            Some(&latest.attempt.sha),
            None,
        )
        .await?;
        let Some(pipeline_id) = promotion else {
            if json {
                push_stage(
                    report,
                    json,
                    "promotion",
                    "wait",
                    "release attempt not yet ready for production promotion",
                    Some(json!({
                        "version": latest.attempt.version,
                        "expected_sha": expected_sha,
                        "attempts": attempts,
                        "canary_status": latest.attempt.canary_status,
                        "release_pipeline_status": latest.attempt.release_pipeline_status,
                        "production_pipeline_status": latest.attempt.production_pipeline_status,
                    })),
                )?;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };

        let refreshed = build_release_status_report(
            db,
            ReleaseStatusQuery {
                project_id: Some(project_id),
                ref_name: Some(ref_name.to_string()),
                sha: Some(latest.attempt.sha.clone()),
                limit: 5,
            },
        )
        .await?;
        let refreshed_latest = refreshed.latest.as_ref().ok_or_else(|| {
            anyhow::anyhow!("release status report missing latest attempt after promotion")
        })?;
        if refreshed_latest
            .attempt
            .production_pipeline_status
            .as_deref()
            == Some("success")
        {
            return Ok(PromotionState {
                main_pipeline_id: refreshed_latest.attempt.release_pipeline_id,
                production_pipeline_id: Some(pipeline_id),
            });
        }
        if json {
            push_stage(
                report,
                json,
                "promotion",
                "wait",
                "production promotion pipeline triggered but not finished yet",
                Some(json!({
                "main_pipeline_id": refreshed_latest.attempt.release_pipeline_id,
                    "production_pipeline_id": pipeline_id,
                    "version": refreshed_latest.attempt.version,
                    "expected_sha": expected_sha,
                    "attempts": attempts,
                })),
            )?;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn perform_github_handoff(
    report: &mut FullPathReport,
    options: &FullPathOptions,
    project_path: &str,
    title: &str,
) -> Result<String> {
    match crate::repo_local::shadow_main_runs(Some(project_path.to_string())).await {
        Ok(runs) if runs.iter().all(|run| run.status != "shadow_failed") && !runs.is_empty() => {
            Ok("shadow_push".to_string())
        }
        Ok(runs) => {
            let detail = runs
                .iter()
                .find(|run| run.status == "shadow_failed")
                .map(|run| run.detail.clone())
                .unwrap_or_else(|| "no matching shadow config found".to_string());
            let body_path = github_pr_body_path(&options.source);
            if let Some(parent) = body_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::write(
                &body_path,
                format!(
                    "Fallback GitHub handoff for {version}\n\nSource: `{source}`\nTarget: `{target}`\nSHA: `{sha}`\nProject: `{project}`\nReason: {detail}\n",
                    version = report.version,
                    source = options.source,
                    target = options.target,
                    sha = report.source_sha,
                    project = project_path,
                    detail = detail
                ),
            )
            .with_context(|| format!("write {}", body_path.display()))?;
            crate::repo_local::open_github_draft_pr(title, &body_path)?;
            Ok("draft_pr_fallback".to_string())
        }
        Err(err) => {
            let body_path = github_pr_body_path(&options.source);
            if let Some(parent) = body_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::write(
                &body_path,
                format!(
                    "Fallback GitHub handoff for {version}\n\nSource: `{source}`\nTarget: `{target}`\nSHA: `{sha}`\nProject: `{project}`\nReason: {detail}\n",
                    version = report.version,
                    source = options.source,
                    target = options.target,
                    sha = report.source_sha,
                    project = project_path,
                    detail = err
                ),
            )
            .with_context(|| format!("write {}", body_path.display()))?;
            crate::repo_local::open_github_draft_pr(title, &body_path)?;
            Ok("draft_pr_fallback".to_string())
        }
    }
}

async fn install_from_artifact(artifact_path: &Path) -> Result<()> {
    let status = tokio::process::Command::new("bash")
        .arg(deploy_lane_script())
        .arg("install-from-artifact")
        .arg(artifact_path)
        .status()
        .await
        .context("run post-merge deploy install lane")?;
    if !status.success() {
        bail!(
            "post-merge deploy lane failed with exit code {:?}",
            status.code()
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct HealthProbeReport {
    ok: bool,
    status_code: u16,
    detail: String,
    body: String,
}

async fn probe_health() -> Result<HealthProbeReport> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("build health probe client")?;
    let response = client.get(health_probe_url()).send().await?;
    let status_code = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok(HealthProbeReport {
        ok: status_code == 200,
        detail: if status_code == 200 {
            "local web health responded 200".to_string()
        } else {
            format!("local web health returned {status_code}")
        },
        status_code,
        body,
    })
}

fn deploy_lane_script() -> PathBuf {
    crate::settings::release_repo_root().join("ops/ci/post-merge-deploy-lane.sh")
}

fn deploy_artifact_path() -> PathBuf {
    crate::settings::release_repo_root().join("target/release/jeryu")
}

fn health_probe_url() -> String {
    "http://127.0.0.1:8787/health".to_string()
}

fn github_pr_body_path(source: &str) -> PathBuf {
    crate::settings::release_repo_root()
        .join("ops/releases/draft")
        .join(source.replace('/', "_"))
        .join("pr-body.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_version_prefers_explicit() {
        assert_eq!(
            resolve_release_version(Some("v1.2.4"), "abcdef1234567890").unwrap(),
            "v1.2.4"
        );
    }

    #[test]
    fn normalizes_refs_for_current_branch_checks() {
        assert_eq!(normalize_ref("refs/heads/main"), "main");
        assert_eq!(normalize_ref("main"), "main");
    }

    #[test]
    fn render_text_includes_summary() {
        let mut report = FullPathReport::new(
            "main".into(),
            "main".into(),
            "v1.2.4".into(),
            "abcdef".into(),
            "root/jeryu".into(),
            false,
        );
        report.success = true;
        report.stages.push(FullPathStage {
            name: "access".into(),
            status: "pass".into(),
            detail: "ok".into(),
            data: None,
        });
        let text = render_full_path_text(&report);
        assert!(text.contains("jeryu release full-path"));
        assert!(text.contains("success:  true"));
        assert!(text.contains("[pass] access"));
    }
}
