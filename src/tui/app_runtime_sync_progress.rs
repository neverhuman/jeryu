use super::*;

pub(super) fn retain_tail(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }

    let mut start = input.len().saturating_sub(max_bytes);
    while !input.is_char_boundary(start) {
        start += 1;
    }

    format!("... (truncated)\n{}", &input[start..])
}

/// Recursively calculate the size of a directory in bytes.
pub(super) async fn dir_size_bytes(path: &std::path::Path) -> i64 {
    let mut total: i64 = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if meta.is_file() {
                    total += meta.len() as i64;
                } else if meta.is_dir() {
                    total += Box::pin(dir_size_bytes(&entry.path())).await;
                }
            }
        }
    }
    total
}

pub(crate) fn build_stage_progress_from_ci_runs(
    runs: &[crate::state::CiJobRun],
) -> Vec<StageProgress> {
    use std::collections::HashMap;
    let mut stage_order: Vec<String> = Vec::new();
    let mut stage_map: HashMap<String, StageProgress> = HashMap::new();

    for run in runs {
        if !stage_map.contains_key(&run.stage) {
            stage_order.push(run.stage.clone());
            stage_map.insert(
                run.stage.clone(),
                StageProgress {
                    stage_name: run.stage.clone(),
                    ..Default::default()
                },
            );
        }
        let entry = stage_map.get_mut(&run.stage).unwrap();
        entry.total_jobs += 1;
        match run.status.as_str() {
            "success" => entry.completed_jobs += 1,
            "running" => entry.running_jobs += 1,
            "failed" | "canceled" => entry.failed_jobs += 1,
            _ => {}
        }
    }

    stage_order
        .into_iter()
        .map(|name| {
            let mut s = stage_map.remove(&name).unwrap();
            s.status = stage_status_str(&s);
            s
        })
        .collect()
}

pub(crate) fn build_stage_progress_from_events(
    events: &[crate::state::JobEvent],
    pipeline_id: i64,
) -> Vec<StageProgress> {
    use std::collections::HashMap;
    let mut stage_order: Vec<String> = Vec::new();
    let mut stage_map: HashMap<String, StageProgress> = HashMap::new();

    for event in events.iter().filter(|e| e.pipeline_id == Some(pipeline_id)) {
        let stage = match event.pool_name.clone() {
            Some(stage) => stage,
            None => "default".to_string(),
        };
        if !stage_map.contains_key(&stage) {
            stage_order.push(stage.clone());
            stage_map.insert(
                stage.clone(),
                StageProgress {
                    stage_name: stage.clone(),
                    ..Default::default()
                },
            );
        }
        let entry = stage_map.get_mut(&stage).unwrap();
        entry.total_jobs += 1;
        match event.status.as_str() {
            "success" => entry.completed_jobs += 1,
            "running" => entry.running_jobs += 1,
            "failed" | "canceled" => entry.failed_jobs += 1,
            _ => {}
        }
    }

    stage_order
        .into_iter()
        .map(|name| {
            let mut s = stage_map.remove(&name).unwrap();
            s.status = stage_status_str(&s);
            s
        })
        .collect()
}

fn stage_status_str(s: &StageProgress) -> String {
    if s.failed_jobs > 0 {
        "failed".into()
    } else if s.running_jobs > 0 {
        "running".into()
    } else if s.completed_jobs == s.total_jobs && s.total_jobs > 0 {
        "success".into()
    } else {
        "pending".into()
    }
}
