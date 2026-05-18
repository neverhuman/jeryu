use super::super::*;
use anyhow::Result;

pub async fn run_next(project_id: i64, ref_name: String) -> Result<()> {
    let db = state::Db::open().await?;

    let pipelines = db
        .list_active_pipelines_for_ref(project_id, &ref_name)
        .await?;
    let release = db.latest_release_attempt(project_id, &ref_name).await?;
    let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);
    let evidence = db.list_evidence_for_ref(project_id, &ref_name, 1).await?;

    println!("━━━ jeryu next — {ref_name} ━━━\n");

    if let Some(rec) = evidence.first() {
        println!("  ● PRIORITY: pipeline failure detected");
        println!(
            "    job={}  stage={}  kind={}  sha={}",
            rec.job_id,
            rec.stage,
            rec.failure_kind,
            &rec.commit_sha[..rec.commit_sha.len().min(12)]
        );
        println!(
            "    Action: jeryu job explain --project-id {project_id} --job-id {}",
            rec.job_id
        );
        println!();
    }

    if !pipelines.is_empty() {
        println!("  ● {} active pipeline(s) for {ref_name}", pipelines.len());
        for p in &pipelines {
            println!(
                "    pipeline={}  status={}  updated={}",
                p.pipeline_id, p.status, p.updated_at
            );
        }
        println!();
    }

    if let Some(rel) = &release {
        let upstream_ok = rel.upstream_status == "success";
        let canary_ok = rel.canary_status == "passed" || rel.canary_status == "skipped";
        if !upstream_ok {
            println!(
                "  ● Release blocked: upstream pipeline status={}",
                rel.upstream_status
            );
            println!(
                "    Action: jeryu release status --project-id {project_id} --ref-name {ref_name}"
            );
        } else if !canary_ok {
            println!(
                "  ⟳ Release in progress: canary_status={}",
                rel.canary_status
            );
            println!(
                "    Action: jeryu release status --project-id {project_id} --ref-name {ref_name}"
            );
        } else {
            println!(
                "  ✓ Release gate OK (sha={})",
                &rel.sha[..rel.sha.len().min(12)]
            );
        }
        println!();
    } else {
        println!("  ○ No release attempt tracked for {ref_name}");
        println!();
    }

    if miss_count > 0 {
        println!("  ● {miss_count} unrepaired selector miss(es) in last 7 days");
        println!("    Action: jeryu test audit --changed <files> --failed <tests> --sha HEAD");
        println!();
    }

    if evidence.is_empty() && pipelines.is_empty() && miss_count == 0 {
        println!("  ✓ No active issues detected for {ref_name}.");
    }

    Ok(())
}

pub async fn run_explain_blocker(entity_type: String, entity_id: i64) -> Result<()> {
    let db = state::Db::open().await?;

    println!("━━━ jeryu explain-blocker {entity_type}:{entity_id} ━━━\n");

    match entity_type.as_str() {
        "job" => {
            if let Some(cap) = db.latest_evidence_by_job_id(entity_id).await? {
                println!(
                    "  job={}  stage={}  ref={}",
                    cap.job_id, cap.stage, cap.ref_name
                );
                println!(
                    "  commit:       {}",
                    &cap.commit_sha[..cap.commit_sha.len().min(12)]
                );
                println!("  failure_kind: {}", cap.failure_kind);
                println!("  exit_code:    {}", cap.exit_code);
                println!("  classified:   {:?}", cap.classify());
                println!("  recovery_advice: {:?}", cap.recommended_recovery());
                println!("  summary:      {}", cap.summary);
                if !cap.repro_script.is_empty() {
                    println!("\n  Repro script:\n    {}", cap.repro_script);
                }
                if !cap.log_snippet.is_empty() {
                    println!("\n  Log (last 10 lines):");
                    for line in cap
                        .log_snippet
                        .lines()
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .take(10)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                    {
                        println!("    {}", line);
                    }
                }
                if let Some(sup) = &cap.superseded_by_sha {
                    println!(
                        "\n  Note: superseded by commit {}",
                        &sup[..sup.len().min(12)]
                    );
                }
            } else {
                println!("  No failure capsule found for job {entity_id}.");
                println!("  Try: jeryu job trace --project-id <id> --job-id {entity_id}");
            }
        }
        "release" => {
            let attempts = db.recent_release_attempts(None, None, 20).await?;
            if let Some(rel) = attempts.iter().find(|r| r.id == entity_id) {
                println!(
                    "  id={}  ref={}  sha={}",
                    rel.id,
                    rel.ref_name,
                    &rel.sha[..rel.sha.len().min(12)]
                );
                println!(
                    "  upstream:     {} (pipeline={:?})",
                    rel.upstream_status, rel.upstream_pipeline_id
                );
                println!(
                    "  canary:       {} (started={:?})",
                    rel.canary_status, rel.canary_started_at
                );
                println!(
                    "  release_pipe: {:?} status={:?}",
                    rel.release_pipeline_id, rel.release_pipeline_status
                );
                println!(
                    "  prod_pipe:    {:?} status={:?}",
                    rel.production_pipeline_id, rel.production_pipeline_status
                );
                println!();
                if rel.upstream_status != "success" {
                    println!(
                        "  BLOCKER: upstream pipeline not green (status={})",
                        rel.upstream_status
                    );
                }
                if rel.canary_status == "running" {
                    println!("  WAITING: canary still running");
                } else if rel.canary_status == "failed" {
                    println!("  BLOCKER: canary failed — {:?}", rel.canary_note);
                }
                if rel.production_pipeline_status.as_deref() == Some("failed") {
                    println!("  BLOCKER: production pipeline failed");
                }
                println!(
                    "\n  Action: jeryu release status --project-id {} --ref-name {}",
                    rel.project_id, rel.ref_name
                );
            } else {
                println!("  No release attempt with id={entity_id} found.");
            }
        }
        "merge" => {
            let since = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
            let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);
            println!("  MR iid: {entity_id}");
            println!("  Selector misses (30d): {miss_count}");
            if miss_count > 0 {
                println!("  BLOCKER: {miss_count} unrepaired test selector miss(es).");
                println!(
                    "  Action:  jeryu test audit --changed <files> --failed <tests> --sha HEAD"
                );
            } else {
                println!("  ✓ No selector misses.");
            }
            println!("\n  For full pipeline/approval status:");
            println!("    jeryu pipeline explain --project-id <id> --pipeline-id <id>");
        }
        other => {
            println!("  Unknown entity type '{other}'. Supported: job | release | merge");
        }
    }

    Ok(())
}
