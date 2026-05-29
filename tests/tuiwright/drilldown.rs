//! Owner: Tuiwright test suite - drilldown matrix
//! Proof: `cargo nextest run --test tuiwright -- drilldown::`
//! Invariants: every `PaneId::panes_for_tab()` entry has a visible anchor and
//! can enter and leave drill-down/fullscreen focus without losing the tab view.

use anyhow::Context;
use jeryu::tui::app::ActiveTab;
use jeryu::tui::focus::PaneId;
use std::time::Duration;
use tuiwright::{Key, Page};

use crate::helpers::*;

fn tab_arg(tab: ActiveTab) -> &'static str {
    match tab {
        ActiveTab::Workflow => "workflow",
        ActiveTab::Mission => "mission",
        ActiveTab::Release => "release",
        ActiveTab::Approvals => "approvals",
        ActiveTab::Jobs => "jobs",
        ActiveTab::Agents => "agents",
        ActiveTab::Tests => "tests",
        ActiveTab::Pools => "pools",
        ActiveTab::Cache => "cache",
        ActiveTab::Evidence => "evidence",
        ActiveTab::Repos => "repos",
        ActiveTab::Bugs => "bugs",
        ActiveTab::Secrets => "secrets",
        ActiveTab::LLMs => "llms",
        ActiveTab::Git => "git",
        ActiveTab::Jankurai => "jankurai",
    }
}

fn pane_anchor(tab: ActiveTab, pane: PaneId) -> String {
    match (tab, pane) {
        (ActiveTab::Workflow, PaneId::WorkflowMissionStrip) => "Mission Control".into(),
        (ActiveTab::Workflow, PaneId::WorkflowPrRail) => "PRs".into(),
        (ActiveTab::Workflow, PaneId::WorkflowPhaseRail) => "Phase".into(),
        (ActiveTab::Workflow, PaneId::WorkflowCanvas) => "Canvas".into(),
        (ActiveTab::Workflow, PaneId::WorkflowMinimap) => "Map".into(),
        (ActiveTab::Workflow, PaneId::WorkflowInspector) => "Inspector".into(),
        (_, PaneId::ActivityLog(_)) => "Activity / Logs".into(),

        (ActiveTab::Mission, PaneId::MissionTopSignal) => "TOP SIGNAL".into(),
        (ActiveTab::Mission, PaneId::MissionReadiness) => "Readiness".into(),
        (ActiveTab::Mission, PaneId::MissionMetrics) => "Autonomy".into(),
        (ActiveTab::Mission, PaneId::MissionAttention) => "Attention Queue".into(),
        (ActiveTab::Mission, PaneId::MissionProofLanes) => "Proof Stack".into(),
        (ActiveTab::Mission, PaneId::MissionActions) => "Next Actions".into(),

        (ActiveTab::Release, PaneId::ReleaseSelector) => "release".into(),
        (ActiveTab::Release, PaneId::ReleasePipeline) => "Release Gate Matrix".into(),
        (ActiveTab::Release, PaneId::ReleaseInspector) => "Inspector".into(),
        (ActiveTab::Release, PaneId::ReleaseRollback) => "Rollback ladder".into(),

        (ActiveTab::Approvals, PaneId::ApprovalsQueue) => "Approvals".into(),
        (ActiveTab::Approvals, PaneId::ApprovalsInspector) => "Inspector".into(),

        (ActiveTab::Jobs, PaneId::JobsRunnerFeed) => "Live Runner Feed".into(),
        (ActiveTab::Jobs, PaneId::JobsProgress) => "Pipeline Progress".into(),
        (ActiveTab::Jobs, PaneId::JobsMatrix) => "Job Matrix".into(),
        (ActiveTab::Jobs, PaneId::JobsInspector) => "Inspector".into(),

        (ActiveTab::Agents, PaneId::AgentsSessions) => "Agent Sessions".into(),
        (ActiveTab::Agents, PaneId::AgentsCockpit) => "Agent Cockpit".into(),
        (ActiveTab::Agents, PaneId::AgentsTimeline) => "Agent Timeline".into(),
        (ActiveTab::Agents, PaneId::AgentsActions) => "Actions / Grants".into(),

        (ActiveTab::Tests, PaneId::TestsBottlenecks) => "Bottlenecks".into(),
        (ActiveTab::Tests, PaneId::TestsHistory) => "History Drill-Down".into(),

        (ActiveTab::Pools, PaneId::PoolsList) => "Runner Pools".into(),
        (ActiveTab::Pools, PaneId::PoolsDetail) => "Pool Detail".into(),

        (ActiveTab::Cache, PaneId::CacheDisk) => "Disk Pressure".into(),
        (ActiveTab::Cache, PaneId::CacheStorage) => "Storage Overview".into(),
        (ActiveTab::Cache, PaneId::CacheGateway) => "Gateway Health".into(),
        (ActiveTab::Cache, PaneId::CacheSingleflight) => "Singleflight Analytics".into(),
        (ActiveTab::Cache, PaneId::CacheTaint) => "Trust & Taint Boundaries".into(),

        (ActiveTab::Evidence, PaneId::EvidenceList) => "Evidence Capsules".into(),
        (ActiveTab::Evidence, PaneId::EvidenceDetail) => "Capsule Detail".into(),

        (ActiveTab::Repos, PaneId::ReposLens) => "Repository Fleet".into(),

        (ActiveTab::Bugs, PaneId::BugsProjects) => "Bug Projects".into(),
        (ActiveTab::Bugs, PaneId::BugsTable) => "Bugs sort".into(),
        (ActiveTab::Bugs, PaneId::BugsInspector) => "Inspector".into(),

        (ActiveTab::Secrets, PaneId::SecretsList) => "Secret Audit Events".into(),
        (ActiveTab::Secrets, PaneId::SecretsDetail) => "Vault Status".into(),

        (ActiveTab::LLMs, PaneId::LLMsPolicyMatrix) => "LLM Policy Matrix".into(),
        (ActiveTab::LLMs, PaneId::LLMsPolicySplit) => "Model Policy Split".into(),

        (ActiveTab::Git, PaneId::GitLedger) => "Git Command Ledger".into(),

        (ActiveTab::Jankurai, PaneId::JankSummary) => "Jankurai Summary".into(),
        (ActiveTab::Jankurai, PaneId::JankStatus) => "Jankurai Status".into(),
        (ActiveTab::Jankurai, PaneId::JankScoreChart) => "Score History".into(),
        (ActiveTab::Jankurai, PaneId::JankBreakdown) => "Last Scan Dimensions".into(),
        (ActiveTab::Jankurai, PaneId::JankIssues) => "Caps / Findings".into(),
        (ActiveTab::Jankurai, PaneId::JankEntryDetail) => "Entry Detail".into(),

        _ => pane.label(),
    }
}

fn click_anchor(page: &Page, anchor: &str) -> anyhow::Result<()> {
    page.wait_for_text(anchor, Duration::from_secs(5))?;
    let match_ = page
        .get_by_text(anchor)
        .resolve_first(&page.screen())
        .with_context(|| format!("expected to locate anchor {anchor:?}"))?;
    let (col, row) = match_.center();
    page.click_cell(col, row)?;
    Ok(())
}

fn drill_and_escape(page: &Page, tab: ActiveTab, pane: PaneId) -> anyhow::Result<()> {
    let anchor = pane_anchor(tab, pane);
    click_anchor(page, &anchor)?;
    if page
        .wait_for_text("[esc]", Duration::from_millis(250))
        .is_err()
    {
        page.press(Key::Enter)?;
        page.wait_for_text("[esc]", Duration::from_secs(5))?;
    }
    page.press(Key::Esc)?;
    page.wait_for_text(&anchor, Duration::from_secs(5))?;
    page.expect_screen()
        .not_to_contain_text("[esc]")
        .with_context(|| format!("esc badge should clear after Esc for {tab:?} / {pane:?}"))?;
    Ok(())
}

#[test]
fn drilldown_matrix_covers_every_tab_and_pane() -> anyhow::Result<()> {
    let _guard = tuiwright_lock();
    for tab in [
        ActiveTab::Workflow,
        ActiveTab::Mission,
        ActiveTab::Release,
        ActiveTab::Approvals,
        ActiveTab::Jobs,
        ActiveTab::Agents,
        ActiveTab::Tests,
        ActiveTab::Pools,
        ActiveTab::Cache,
        ActiveTab::Evidence,
        ActiveTab::Repos,
        ActiveTab::Bugs,
        ActiveTab::Secrets,
        ActiveTab::LLMs,
        ActiveTab::Git,
        ActiveTab::Jankurai,
    ] {
        let page = spawn_interactive_tui_size(tab_arg(tab), 240, 60)?;
        let panes = PaneId::panes_for_tab(tab);
        let default_anchor = pane_anchor(tab, panes[0]);
        page.wait_for_text(&default_anchor, Duration::from_secs(5))?;

        for &pane in panes {
            drill_and_escape(&page, tab, pane)?;
        }
    }
    Ok(())
}
