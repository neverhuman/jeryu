use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use crate::state::Db;

pub async fn abort_with_quarantine_capsule(
    db: &Db,
    job_id: i64,
    project_id: Option<i64>,
    stage: &str,
    quarantine_marker: &Path,
    log_buffer: &Arc<Mutex<Vec<u8>>>,
    script_path: &str,
) -> Result<()> {
    let reason = std::fs::read_to_string(quarantine_marker).unwrap_or_default();
    let log_snippet = String::from_utf8_lossy(&log_buffer.lock().unwrap()).to_string();
    let capsule = crate::capsule::FailureCapsule::capture(
        job_id,
        project_id.unwrap_or(0),
        stage,
        999,
        format!("🚨 QUARANTINED: {}\n\nLogs:\n{}", reason, log_snippet),
        &format!("bash {}", script_path),
    );
    db.insert_evidence_capsule("quarantine_capsule", &capsule)
        .await?;
    db.append_event(
        "quarantine_capsule",
        project_id,
        Some(job_id),
        "jeryu-exec",
        &capsule.to_json(),
    )
    .await?;
    std::process::exit(1);
}

pub async fn abort_with_failure_capsule(
    db: &Db,
    job_id: i64,
    project_id: Option<i64>,
    stage: &str,
    exit_code: i32,
    log_buffer: &Arc<Mutex<Vec<u8>>>,
    script_path: &str,
) -> Result<()> {
    let log_snippet = String::from_utf8_lossy(&log_buffer.lock().unwrap()).to_string();
    let capsule = crate::capsule::FailureCapsule::capture(
        job_id,
        project_id.unwrap_or(0),
        stage,
        exit_code,
        log_snippet,
        &format!("bash {}", script_path),
    );

    db.insert_evidence_capsule("failure_capsule", &capsule)
        .await?;
    db.append_event(
        "failure_capsule",
        project_id,
        Some(job_id),
        "jeryu-exec",
        &capsule.to_json(),
    )
    .await?;

    std::process::exit(exit_code);
}
