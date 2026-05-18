use super::*;

pub(super) fn demo_runner_feeds(now_str: &str) -> Vec<RunnerFeed> {
    vec![
        RunnerFeed {
            runner_name: "trusted-01".into(),
            job_id: 9_002,
            job_name: "build-image".into(),
            pipeline_id: 8_013,
            status: "running".into(),
            elapsed_secs: 134.0,
            log_tail: "[2026-05-02 23:04:12] Compiling signal-router v0.4.2\n[2026-05-02 23:04:13] Compiling tokio v1.40\n[2026-05-02 23:04:14]   warning: missing import `std::io`\n[2026-05-02 23:04:15] Compiling serde v1.0\n[2026-05-02 23:04:16] Compiling log v0.4\n[2026-05-02 23:04:17] Finished `release` profile in 2m14s".into(),
            updated_at: now_str.to_string(),
        },
        RunnerFeed {
            runner_name: "trusted-02".into(),
            job_id: 9_005,
            job_name: "integration-tests".into(),
            pipeline_id: 8_013,
            status: "running".into(),
            elapsed_secs: 87.0,
            log_tail: "[2026-05-02 23:04:30] Running integration test suite...\n[2026-05-02 23:04:31] test integration::cache_warmer_info_routes_to_all_and_cache ... ok\n[2026-05-02 23:04:32] test integration::auth_error_routes_to_alerts_and_security ... FAILED\n[2026-05-02 23:04:33]   Error: route mismatch on severity filter\n[2026-05-02 23:04:34] test integration::batch_routing_preserves_signal_count ... ok".into(),
            updated_at: now_str.to_string(),
        },
        RunnerFeed {
            runner_name: "security-01".into(),
            job_id: 9_004,
            job_name: "security-gate".into(),
            pipeline_id: 8_013,
            status: "failed".into(),
            elapsed_secs: 45.0,
            log_tail: "[2026-05-02 23:03:50] Running security scan...\n[2026-05-02 23:03:52] Checking artifact signatures...\n[2026-05-02 23:03:55] ERROR: Artifact verification timed out\n[2026-05-02 23:03:55] FATAL: security gate failed".into(),
            updated_at: now_str.to_string(),
        },
    ]
}

pub(super) fn demo_pipeline_progress_view(now_str: &str) -> PipelineProgressView {
    PipelineProgressView {
        pipeline_id: 8_013,
        ref_name: "main".into(),
        sha_short: "9c3f2d4e".into(),
        stages: vec![
            StageProgress {
                stage_name: "build".into(),
                total_jobs: 2,
                completed_jobs: 2,
                running_jobs: 0,
                failed_jobs: 0,
                status: "success".into(),
                avg_duration_secs: Some(180.0),
                elapsed_secs: Some(134.0),
            },
            StageProgress {
                stage_name: "test".into(),
                total_jobs: 3,
                completed_jobs: 1,
                running_jobs: 1,
                failed_jobs: 0,
                status: "running".into(),
                avg_duration_secs: Some(300.0),
                elapsed_secs: Some(87.0),
            },
            StageProgress {
                stage_name: "security".into(),
                total_jobs: 2,
                completed_jobs: 0,
                running_jobs: 0,
                failed_jobs: 1,
                status: "failed".into(),
                avg_duration_secs: Some(60.0),
                elapsed_secs: Some(45.0),
            },
            StageProgress {
                stage_name: "deploy".into(),
                total_jobs: 1,
                completed_jobs: 0,
                running_jobs: 0,
                failed_jobs: 0,
                status: "pending".into(),
                avg_duration_secs: Some(120.0),
                elapsed_secs: None,
            },
            StageProgress {
                stage_name: "e2e".into(),
                total_jobs: 2,
                completed_jobs: 0,
                running_jobs: 1,
                failed_jobs: 0,
                status: "running".into(),
                avg_duration_secs: Some(240.0),
                elapsed_secs: Some(87.0),
            },
        ],
        overall_pct: 47,
        eta_remaining_secs: Some(492),
        eta_confidence: "medium".into(),
        wall_clock_secs: 862,
        started_at: Some(now_str.to_string()),
    }
}

pub(super) fn demo_release_stages() -> ReleaseStageSnapshot {
    ReleaseStageSnapshot {
        plan: vec![ReleaseStageCard {
            label: "PR #142".into(),
            agent_id: "claude-opus-4-7".into(),
            age: "2m".into(),
        }],
        build: vec![
            ReleaseStageCard {
                label: "PR #138".into(),
                agent_id: "codex".into(),
                age: "5m".into(),
            },
            ReleaseStageCard {
                label: "PR #140".into(),
                agent_id: "claude-sonnet-4-6".into(),
                age: "3m".into(),
            },
        ],
        proof: vec![ReleaseStageCard {
            label: "PR #135".into(),
            agent_id: "gpt-5".into(),
            age: "11m".into(),
        }],
        canary: vec![ReleaseStageCard {
            label: "v3.0.1-rc.1".into(),
            agent_id: "release-bot".into(),
            age: "47m".into(),
        }],
        stable: vec![ReleaseStageCard {
            label: "v3.0.0".into(),
            agent_id: "release-bot".into(),
            age: "3d".into(),
        }],
    }
}

pub(super) fn demo_approvals_queue() -> Vec<PendingApproval> {
    vec![
        PendingApproval {
            pr_number: 142,
            title: "feat: agent-first release process".into(),
            agent_id: "claude-opus-4-7".into(),
            risk_tier: 3,
            ci_status: "green".into(),
            age: "2m".into(),
            head_sha: "abc123def456".into(),
        },
        PendingApproval {
            pr_number: 140,
            title: "fix: VTI receipt expiry check".into(),
            agent_id: "claude-sonnet-4-6".into(),
            risk_tier: 1,
            ci_status: "running".into(),
            age: "3m".into(),
            head_sha: "789xyz012abc".into(),
        },
    ]
}
