use super::*;

pub async fn build_release_status_report(
    db: &Db,
    query: ReleaseStatusQuery,
) -> Result<ReleaseStatusReport> {
    let recent = if let Some(sha) = &query.sha {
        let mut attempts = Vec::new();
        if let Some(project_id) = query.project_id {
            if let Some(attempt) = db
                .get_release_attempt(project_id, query.ref_name.as_deref().unwrap_or("main"), sha)
                .await?
            {
                attempts.push(attempt);
            }
        } else {
            attempts = db
                .recent_release_attempts(None, query.ref_name.as_deref(), query.limit as i64)
                .await?;
            attempts.retain(|attempt| attempt.sha == *sha);
        }
        attempts
    } else {
        db.recent_release_attempts(
            query.project_id,
            query.ref_name.as_deref(),
            query.limit as i64,
        )
        .await?
    };

    let latest = recent.first().cloned().map(view_attempt).transpose()?;
    let recent = recent
        .into_iter()
        .map(view_attempt)
        .collect::<Result<Vec<_>>>()?;
    Ok(ReleaseStatusReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id: query.project_id,
        ref_name: query.ref_name,
        sha: query.sha,
        limit: query.limit,
        total_attempts: recent.len(),
        latest,
        recent,
    })
}

pub fn summarize_release_attempt(view: &ReleaseAttemptView) -> String {
    let attempt = &view.attempt;
    let upstream = format!("upstream={}", attempt.upstream_status);
    let release_pipeline = match attempt.release_pipeline_id {
        Some(id) => format!("release_pipeline={id}"),
        None => "release_pipeline=none".to_string(),
    };
    let production_pipeline = match attempt.production_pipeline_id {
        Some(id) => format!("production_pipeline={id}"),
        None => "production_pipeline=none".to_string(),
    };
    let canary = format!("canary={}", attempt.canary_status);
    let evidence = view
        .gate_canary_e2e_path
        .rsplit('/')
        .next()
        .unwrap_or(&view.gate_canary_e2e_path);
    format!(
        "{} {} [{}] {} {} {} {} {}",
        attempt.ref_name,
        attempt.version,
        view.canary_state,
        upstream,
        release_pipeline,
        production_pipeline,
        canary,
        evidence
    )
}

pub fn summarize_release_report(report: &ReleaseStatusReport) -> String {
    if let Some(latest) = &report.latest {
        summarize_release_attempt(latest)
    } else {
        "no release attempts found".to_string()
    }
}

pub fn render_release_status_text(report: &ReleaseStatusReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;

    let _ = writeln!(out, "━━━ jeryu release status ━━━");
    let _ = writeln!(
        out,
        "  Scope:      {}",
        release_scope(&ReleaseStatusQuery {
            project_id: report.project_id,
            ref_name: report.ref_name.clone(),
            sha: report.sha.clone(),
            limit: report.limit,
        })
    );
    let _ = writeln!(out, "  Generated:  {}", report.generated_at);
    let _ = writeln!(out, "  Window:     latest {} attempt(s)", report.limit);
    let _ = writeln!(out);

    if let Some(latest) = &report.latest {
        let attempt = &latest.attempt;
        let _ = writeln!(out, "  Latest:");
        let _ = writeln!(out, "    Version:   {}", attempt.version);
        let _ = writeln!(out, "    SHA:       {}", attempt.sha);
        let _ = writeln!(
            out,
            "    Active:    {} (upstream {:?}, release {:?}, prod {:?})",
            attempt.version,
            attempt.upstream_pipeline_id,
            attempt.release_pipeline_id,
            attempt.production_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Upstream:  {} (pipeline {:?})",
            attempt.upstream_status, attempt.upstream_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Release:   {} (pipeline {:?})",
            attempt
                .release_pipeline_status
                .as_deref()
                .unwrap_or("(not triggered)"),
            attempt.release_pipeline_id
        );
        let _ = writeln!(
            out,
            "    Prod:      {} (pipeline {:?})",
            attempt
                .production_pipeline_status
                .as_deref()
                .unwrap_or("(not triggered)"),
            attempt.production_pipeline_id
        );
        let _ = writeln!(out, "    Canary:    {}", attempt.canary_status);
        let _ = writeln!(out, "    State:     {}", latest.canary_state);
        let _ = writeln!(out, "    Eligible:  {}", latest.eligibility);
        let _ = writeln!(
            out,
            "    Phase:     {}",
            latest.phase.as_deref().unwrap_or("(unknown)")
        );
        let _ = writeln!(
            out,
            "    StateFile: {}",
            latest.state_status.as_deref().unwrap_or("(missing)")
        );
        let _ = writeln!(
            out,
            "    Gates:     remote={} telemetry={} e2e={} telemetry_diag={} identity_ok={}",
            latest.has_remote_gate,
            latest.has_telemetry_gate,
            latest.has_e2e_gate,
            latest.has_telemetry_diag,
            latest.release_identity_ok
        );
        let _ = writeln!(
            out,
            "    URL:       {}",
            latest.canary_public_url.as_deref().unwrap_or("(pending)")
        );
        let _ = writeln!(
            out,
            "    Started:   {}",
            attempt
                .canary_started_at
                .as_deref()
                .unwrap_or("(not started)")
        );
        let _ = writeln!(
            out,
            "    Finished:  {}",
            attempt
                .canary_finished_at
                .as_deref()
                .unwrap_or("(not finished)")
        );
        let _ = writeln!(
            out,
            "    Note:      {}",
            latest.detail.as_deref().unwrap_or("(none)")
        );
        let _ = writeln!(out, "    Evidence:  {}", latest.canary_state_path);
        let _ = writeln!(out, "    Release:   {}", latest.release_dir);
        let _ = writeln!(out);
    } else {
        let _ = writeln!(out, "  Latest:     (no release attempts found)");
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "  Recent attempts:");
    if report.recent.is_empty() {
        let _ = writeln!(out, "    (none)");
    } else {
        for attempt in &report.recent {
            let a = &attempt.attempt;
            let _ = writeln!(
                out,
                "    [{}] project={} ref={} sha={} version={} upstream={} release={} prod={} canary={} phase={}",
                attempt.canary_state,
                a.project_id,
                a.ref_name,
                a.sha,
                a.version,
                a.upstream_status,
                a.release_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                a.production_pipeline_status
                    .as_deref()
                    .unwrap_or("not-triggered"),
                a.canary_status,
                attempt.phase.as_deref().unwrap_or("unknown"),
            );
        }
    }

    out
}

pub async fn render_release_status(db: &Db, query: ReleaseStatusQuery, json: bool) -> Result<()> {
    let report = build_release_status_report(db, query).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_release_status_text(&report));
    }
    Ok(())
}

pub async fn watch_release_status(
    db: &Db,
    query: ReleaseStatusQuery,
    json: bool,
    interval_secs: u64,
) -> Result<()> {
    use std::io::{self, Write};
    use tokio::time::{Duration, sleep};

    let mut stdout = io::stdout();
    loop {
        let report = build_release_status_report(db, query.clone()).await?;
        write!(stdout, "\x1b[2J\x1b[H")?;
        if json {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&report)?)?;
        } else {
            write!(stdout, "{}", render_release_status_text(&report))?;
        }
        stdout.flush()?;

        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = sleep(Duration::from_secs(interval_secs)) => {}
        }
    }
    Ok(())
}
