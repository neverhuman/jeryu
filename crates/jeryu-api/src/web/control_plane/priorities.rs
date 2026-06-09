use super::*;

pub(crate) struct PriorityInputs<'a> {
    pub(crate) repos: &'a [ControlRepo],
    pub(crate) prs: &'a [ControlPullRequest],
    pub(crate) checks: &'a [ControlCheckRun],
    pub(crate) runners: &'a RunnerFabricResponse,
    pub(crate) artifacts: &'a ArtifactLatestResponse,
    pub(crate) mirror: &'a RemoteStatusResponse,
    pub(crate) codegraph: &'a CodegraphControlSummary,
    pub(crate) tool_build: &'a ToolBuildControlSummary,
}

struct PriorityDraft<'a> {
    id: String,
    title: String,
    severity: InsightSeverity,
    score: u32,
    owner: &'a str,
    proof_lane: &'a str,
    recommended_action: &'a str,
    evidence: Vec<String>,
    source_links: Vec<SourceLink>,
    state: EvidenceState,
}

pub(crate) fn priority_insights(input: PriorityInputs<'_>) -> Vec<PriorityInsight> {
    let PriorityInputs {
        repos,
        prs,
        checks,
        runners,
        artifacts,
        mirror,
        codegraph,
        tool_build,
    } = input;
    let mut insights = Vec::new();
    for pr in prs {
        if pr.checks.missing {
            insights.push(priority(PriorityDraft {
                id: format!(
                    "pr-{}-{}-checks-missing",
                    pr.repo.replace('/', "-"),
                    pr.number
                ),
                title: format!("PR #{} has no head checks", pr.number),
                severity: InsightSeverity::High,
                score: 840,
                owner: "forge-api",
                proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
                recommended_action:
                    "create or refresh check-runs for the PR head before merge evaluation",
                evidence: vec![
                    format!("repo={}", pr.repo),
                    format!("head_sha={}", pr.head_sha),
                    "missing checks are unsafe evidence, not success".to_string(),
                ],
                source_links: pr.source_links.clone(),
                state: EvidenceState::Missing,
            }));
        }
    }
    let failing = checks
        .iter()
        .filter(|check| check.state == EvidenceState::Failed)
        .collect::<Vec<_>>();
    if !failing.is_empty() {
        insights.push(priority(PriorityDraft {
            id: "ci-failing-checks".to_string(),
            title: format!("{} failing check run(s)", failing.len()),
            severity: InsightSeverity::High,
            score: 780,
            owner: "forge-api",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action: "inspect failing check-run evidence and route repair through typed errors",
            evidence: failing
                .iter()
                .take(5)
                .map(|check| format!("{} {} {}", check.repo, check.name, check.head_sha))
                .collect(),
            source_links: failing
                .iter()
                .take(5)
                .map(|check| SourceLink {
                    label: check.name.clone(),
                    url: format!("/api/v1/ci/runs/{}/evidence", check.id),
                })
                .collect(),
            state: EvidenceState::Failed,
        }));
    }
    if artifacts.state == EvidenceState::Missing {
        insights.push(priority(PriorityDraft {
            id: "artifacts-latest-missing".to_string(),
            title: "Latest artifacts are absent".to_string(),
            severity: InsightSeverity::Medium,
            score: 640,
            owner: "release-security",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "record artifact evidence or keep the absence explicit for release decisions",
            evidence: vec![artifacts.latest_release.reason.clone()],
            source_links: vec![SourceLink {
                label: "release receipt".to_string(),
                url: ARTIFACT_DOCS.to_string(),
            }],
            state: EvidenceState::Missing,
        }));
    }
    if mirror.state == EvidenceState::Missing {
        insights.push(priority(PriorityDraft {
            id: "github-mirror-missing".to_string(),
            title: "GitHub mirror evidence unavailable".to_string(),
            severity: InsightSeverity::Medium,
            score: 600,
            owner: "forge-api",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "treat mirror state as missing until a read-only adapter supplies fresh evidence",
            evidence: vec![mirror.divergence.reason.clone()],
            source_links: vec![SourceLink {
                label: "agent native standard".to_string(),
                url: MIRROR_DOCS.to_string(),
            }],
            state: EvidenceState::Missing,
        }));
    }
    if runners.local.offline_runners > 0 {
        insights.push(priority(PriorityDraft {
            id: "runner-offline-capacity".to_string(),
            title: format!(
                "{} runner(s) offline or fenced",
                runners.local.offline_runners
            ),
            severity: InsightSeverity::High,
            score: 760,
            owner: "ci-runtime",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "repair runner registration or drain evidence before scheduling more work",
            evidence: vec![format!(
                "online={} offline={} active_slots={}",
                runners.local.online_runners,
                runners.local.offline_runners,
                runners.local.active_slots
            )],
            source_links: Vec::new(),
            state: EvidenceState::Failed,
        }));
    }
    if codegraph.state != EvidenceState::Fresh {
        insights.push(priority(PriorityDraft {
            id: "codegraph-index-missing".to_string(),
            title: "Codegraph index is not fresh".to_string(),
            severity: InsightSeverity::Low,
            score: 360,
            owner: "rust-ci",
            proof_lane: "bash ops/ci/codegraph-oracle.sh",
            recommended_action: "rerun the codegraph oracle lane before relying on impact analysis",
            evidence: vec![codegraph.reason.clone()],
            source_links: vec![SourceLink {
                label: "codegraph oracle".to_string(),
                url: "docs/codegraph-oracle.md".to_string(),
            }],
            state: codegraph.state.clone(),
        }));
    }
    if tool_build.cluster_count > 0 {
        insights.push(priority(PriorityDraft {
            id: "tool-build-clusters-ready".to_string(),
            title: format!(
                "{} repeated-code cluster(s) ready",
                tool_build.cluster_count
            ),
            severity: InsightSeverity::Low,
            score: 300,
            owner: "rust-ci",
            proof_lane: "bash ops/ci/codegraph-tool-build.sh",
            recommended_action:
                "review ranked clusters for tool-building extraction or record ignore feedback",
            evidence: tool_build
                .top_clusters
                .iter()
                .map(|cluster| cluster.insight.clone())
                .collect(),
            source_links: vec![SourceLink {
                label: "tool-build clusters".to_string(),
                url: "/api/v1/codegraph/tool-build/clusters".to_string(),
            }],
            state: EvidenceState::Fresh,
        }));
    }
    if repos.is_empty() {
        insights.push(priority(PriorityDraft {
            id: "no-local-repos".to_string(),
            title: "No local repositories imported".to_string(),
            severity: InsightSeverity::Info,
            score: 120,
            owner: "forge-api",
            proof_lane: "cargo test -p jeryu-api --features web --jobs 40 control_plane",
            recommended_action:
                "import a local repository before expecting PR, CI, or graph evidence",
            evidence: vec!["local ForgeCore repository list is empty".to_string()],
            source_links: vec![SourceLink {
                label: "local runtime".to_string(),
                url: "README.md#local-live-runtime".to_string(),
            }],
            state: EvidenceState::Missing,
        }));
    }
    insights.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    insights
}

fn priority(draft: PriorityDraft<'_>) -> PriorityInsight {
    PriorityInsight {
        id: draft.id,
        title: draft.title,
        severity: draft.severity,
        score: draft.score,
        confidence: 1.0,
        owner: draft.owner.to_string(),
        proof_lane: draft.proof_lane.to_string(),
        recommended_action: draft.recommended_action.to_string(),
        evidence: draft.evidence,
        source_links: draft.source_links,
        state: draft.state,
        rules_version: RULES_VERSION.to_string(),
    }
}
