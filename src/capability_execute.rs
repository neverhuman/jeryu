use super::*;

pub(crate) async fn fetch_capsule(job_id: i64) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    match db.latest_evidence_by_job_id(job_id).await {
        Ok(Some(cap)) => CapabilityResponse {
            success: true,
            message: "Capsule retrieved".into(),
            data: serde_json::to_value(cap).ok(),
        },
        Ok(None) => err(&format!("no capsule found for job_id={}", job_id)),
        Err(e) => err(&format!("db error: {}", e)),
    }
}

pub(crate) async fn run_tests(
    project_id: i64,
    target_ref: String,
    test_scope: String,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    let ts = chrono::Utc::now().timestamp_millis();
    let branch = format!("{}-ci-{}", target_ref, ts);

    if let Err(e) = client.create_branch(project_id, &branch, &target_ref).await {
        return err(&format!("create_branch: {}", e));
    }
    let yaml = match dynamic_ci_yaml(&test_scope) {
        Ok(yaml) => yaml,
        Err(e) => return err(&format!("ci template: {e}")),
    };
    let commit_sha = match client
        .commit_actions_with_sha(
            project_id,
            &branch,
            &format!("DAG injection for {}", test_scope),
            &[("update", ".gitlab-ci.yml", yaml.as_str())], // allowlist: not a DB query
        )
        .await
    {
        Ok(sha) => sha,
        Err(e) => return err(&format!("update_file: {}", e)), // allowlist: not a DB query
    };
    let pipeline_id = match client.trigger_pipeline(project_id, &branch, vec![]).await {
        Ok(pipeline_id) => pipeline_id,
        Err(e) => return err(&format!("trigger_pipeline: {}", e)),
    };
    let grant = record_branch_capability_grant(
        &db,
        "RunTests",
        "run_tests",
        ctx,
        project_id,
        &branch,
        Some(&target_ref),
        Some(&commit_sha),
        serde_json::json!({
            "project_id": project_id,
            "target_ref": target_ref,
            "test_scope": test_scope,
            "branch": branch,
            "commit_sha": commit_sha.clone(),
            "pipeline_id": pipeline_id,
        }),
    )
    .await
    .ok();
    CapabilityResponse {
        success: true,
        message: format!("ephemeral test branch created: {}", branch),
        data: Some(
            serde_json::json!({"branch": branch, "scope": test_scope, "pipeline_id": pipeline_id, "grant_id": grant}),
        ),
    }
}

pub(crate) async fn propose_patch(
    project_id: i64,
    branch_name: String,
    base_ref: String,
    commit_message: String,
    modifications: Vec<FileModification>,
    mr_title: Option<String>,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    if let Err(e) = client
        .create_branch(project_id, &branch_name, &base_ref)
        .await
    {
        return err(&format!("create_branch: {}", e));
    }

    let tuples: Vec<(&str, &str, &str)> = modifications
        .iter()
        .map(|m| ("update", m.file_path.as_str(), m.content.as_str()))
        .collect();

    let commit_sha = match client
        .commit_actions_with_sha(project_id, &branch_name, &commit_message, &tuples)
        .await
    {
        Ok(sha) => sha,
        Err(e) => return err(&format!("commit_actions: {}", e)),
    };

    let title = match mr_title {
        Some(t) => t,
        None => commit_message.clone(),
    };
    match client
        .create_merge_request(project_id, &branch_name, &base_ref, &title, "")
        .await
    {
        Ok(mr) => {
            let grant = record_branch_capability_grant(
                &db,
                "ProposePatch",
                "propose_patch",
                ctx,
                project_id,
                &branch_name,
                Some(&base_ref),
                Some(&commit_sha),
                serde_json::json!({
                    "project_id": project_id,
                    "branch": branch_name,
                    "base_ref": base_ref,
                    "commit_sha": commit_sha.clone(),
                    "mr_iid": mr.iid,
                    "mr_url": mr.web_url,
                    "files_changed": modifications.len(),
                }),
            )
            .await
            .ok();
            CapabilityResponse {
                success: true,
                message: format!("MR !{} created on branch {}", mr.iid, branch_name),
                data: Some(serde_json::json!({
                    "branch": branch_name,
                    "mr_iid": mr.iid,
                    "mr_url": mr.web_url,
                    "grant_id": grant,
                })),
            }
        }
        Err(e) => err(&format!("create_merge_request: {}", e)),
    }
}

pub(crate) async fn race_patches(
    project_id: i64,
    base_branch: String,
    commit_message: String,
    hypotheses: Vec<HypothesisPatch>,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    if hypotheses.is_empty() {
        return err("hypotheses list is empty");
    }

    let mut branch_pipeline_pairs: Vec<(String, Option<i64>)> = Vec::new();
    let mut grant_ids: Vec<String> = Vec::new();
    for h in &hypotheses {
        let branch = format!("{}-race-{}", base_branch, h.branch_suffix);
        if client
            .create_branch(project_id, &branch, &base_branch)
            .await
            .is_err()
        {
            continue;
        }
        let tuples: Vec<(&str, &str, &str)> = h
            .modifications
            .iter()
            .map(|m| ("update", m.file_path.as_str(), m.content.as_str())) // allowlist: not a DB query
            .collect();
        let Ok(commit_sha) = client
            .commit_actions_with_sha(project_id, &branch, &commit_message, &tuples)
            .await
        else {
            continue;
        };
        let pid = client
            .trigger_pipeline(project_id, &branch, vec![])
            .await
            .ok();
        if let Ok(grant_id) = record_branch_capability_grant(
            &db,
            "RacePatches",
            "race_patches",
            ctx,
            project_id,
            &branch,
            Some(&base_branch),
            Some(&commit_sha),
            serde_json::json!({
                "project_id": project_id,
                "base_branch": base_branch,
                "branch": branch,
                "branch_suffix": h.branch_suffix,
                "pipeline_id": pid,
                "commit_sha": commit_sha.clone(),
                "files_changed": h.modifications.len(),
            }),
        )
        .await
        {
            grant_ids.push(grant_id);
        }
        branch_pipeline_pairs.push((branch, pid));
    }

    if branch_pipeline_pairs.is_empty() {
        return err("failed to create any hypothesis branches");
    }

    CapabilityResponse {
        success: true,
        message: format!(
            "{} hypothesis branches launched; monitor pipelines for winner",
            branch_pipeline_pairs.len()
        ),
        data: Some(serde_json::json!({
            "branches": branch_pipeline_pairs.iter().map(|(b, pid)| serde_json::json!({"branch": b, "pipeline_id": pid})).collect::<Vec<_>>(),
            "grant_ids": grant_ids,
            "status": "racing",
            "note": "Poll pipeline status to determine winner; losing branches will need manual cleanup or implement PollRaceResult."
        })),
    }
}

pub(crate) async fn request_merge(
    project_id: i64,
    mr_iid: i64,
    source_branch: String,
    target_branch: String,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match client
        .accept_merge_request(project_id, mr_iid)
        .await
    {
        Ok(_) => CapabilityResponse {
            success: true,
            message: format!("merge request !{} requested", mr_iid),
            data: Some(serde_json::json!({"mr_iid": mr_iid, "source_branch": source_branch, "target_branch": target_branch})),
        },
        Err(e) => err(&format!("request_merge: {}", e)),
    }
}

fn err(msg: &str) -> CapabilityResponse {
    CapabilityResponse {
        success: false,
        message: msg.to_string(),
        data: None,
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_branch_capability_grant(
    db: &crate::state::Db,
    intent_type: &str,
    action_id: &str,
    ctx: &CapabilityContext,
    project_id: i64,
    branch_name: &str,
    target_ref: Option<&str>,
    new_sha: Option<&str>,
    payload: serde_json::Value,
) -> anyhow::Result<String> {
    let grant_id = format!("grant-{}", uuid::Uuid::new_v4());
    let ref_name = qualify_branch_ref(branch_name);
    let payload = serde_json::json!({
        "protocol_version": ctx.protocol_version,
        "request_id": ctx.request_id,
        "actor": ctx.actor,
        "bridge_mode": ctx.bridge_mode,
        "scope": {
            "project_id": project_id,
            "ref_name": ref_name,
            "target_ref": target_ref,
            "new_sha": new_sha,
        },
        "intent_payload": payload,
    });
    let payload = serde_json::to_string(&payload)?;
    let intent_id = db
        .record_capability_intent(crate::state::NewCapabilityIntent {
            request_id: &ctx.request_id,
            intent_type,
            action_id,
            project_id: Some(project_id),
            ref_name: Some(&ref_name),
            target_ref,
            actor: &ctx.actor,
            status: "executed",
            payload: &payload,
        })
        .await?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339();
    db.approve_capability_grant(crate::state::NewCapabilityGrant {
        intent_id,
        grant_id: &grant_id,
        action_id,
        project_id: Some(project_id),
        ref_name: &ref_name,
        new_sha,
        required_grant: "agent_task",
        status: "approved",
        expires_at: &expires_at,
        payload: &payload,
    })
    .await?;
    Ok(grant_id)
}

fn qualify_branch_ref(branch_name: &str) -> String {
    if branch_name.starts_with("refs/") {
        branch_name.to_string()
    } else {
        format!("refs/heads/{branch_name}")
    }
}
