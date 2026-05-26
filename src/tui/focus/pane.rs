use crate::tui::app::ActiveTab;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
    /// Global fleet bar - visible on every tab, above the content area.
    FleetBar,
    WorkflowMissionStrip,
    WorkflowPrRail,
    WorkflowPhaseRail,
    WorkflowCanvas,
    WorkflowMinimap,
    WorkflowInspector,
    ActivityLog(ActiveTab),
    MissionTopSignal,
    MissionReadiness,
    MissionMetrics,
    MissionAttention,
    MissionProofLanes,
    MissionActions,
    ReleaseSelector,
    ReleasePipeline,
    ReleaseInspector,
    ReleaseRollback,
    ApprovalsQueue,
    ApprovalsInspector,
    JobsRunnerFeed,
    JobsProgress,
    JobsMatrix,
    JobsInspector,
    AgentsSessions,
    AgentsCockpit,
    AgentsTimeline,
    AgentsActions,
    TestsBottlenecks,
    TestsHistory,
    PoolsList,
    PoolsDetail,
    CacheDisk,
    CacheStorage,
    CacheGateway,
    CacheSingleflight,
    CacheTaint,
    EvidenceList,
    EvidenceDetail,
    ReposLens,
    BugsProjects,
    BugsTable,
    BugsInspector,
    SecretsList,
    SecretsDetail,
    LLMsPolicyMatrix,
    LLMsPolicySplit,
    GitLedger,
    JankSummary,
    JankStatus,
    JankScoreChart,
    JankBreakdown,
    JankIssues,
    JankEntryDetail,
}

impl PaneId {
    /// Returns `true` for panes that appear on every tab, such as the fleet bar.
    pub fn is_global(self) -> bool {
        matches!(self, PaneId::FleetBar)
    }

    pub fn tab(self) -> ActiveTab {
        match self {
            PaneId::FleetBar => ActiveTab::Workflow,
            PaneId::WorkflowMissionStrip
            | PaneId::WorkflowPrRail
            | PaneId::WorkflowPhaseRail
            | PaneId::WorkflowCanvas
            | PaneId::WorkflowMinimap
            | PaneId::WorkflowInspector
            | PaneId::ActivityLog(ActiveTab::Workflow) => ActiveTab::Workflow,
            PaneId::ActivityLog(tab) => tab,
            PaneId::MissionTopSignal
            | PaneId::MissionReadiness
            | PaneId::MissionMetrics
            | PaneId::MissionAttention
            | PaneId::MissionProofLanes
            | PaneId::MissionActions => ActiveTab::Mission,
            PaneId::ReleaseSelector
            | PaneId::ReleasePipeline
            | PaneId::ReleaseInspector
            | PaneId::ReleaseRollback => ActiveTab::Release,
            PaneId::ApprovalsQueue | PaneId::ApprovalsInspector => ActiveTab::Approvals,
            PaneId::JobsRunnerFeed
            | PaneId::JobsProgress
            | PaneId::JobsMatrix
            | PaneId::JobsInspector => ActiveTab::Jobs,
            PaneId::AgentsSessions
            | PaneId::AgentsCockpit
            | PaneId::AgentsTimeline
            | PaneId::AgentsActions => ActiveTab::Agents,
            PaneId::TestsBottlenecks | PaneId::TestsHistory => ActiveTab::Tests,
            PaneId::PoolsList | PaneId::PoolsDetail => ActiveTab::Pools,
            PaneId::CacheDisk
            | PaneId::CacheStorage
            | PaneId::CacheGateway
            | PaneId::CacheSingleflight
            | PaneId::CacheTaint => ActiveTab::Cache,
            PaneId::EvidenceList | PaneId::EvidenceDetail => ActiveTab::Evidence,
            PaneId::ReposLens => ActiveTab::Repos,
            PaneId::BugsProjects | PaneId::BugsTable | PaneId::BugsInspector => ActiveTab::Bugs,
            PaneId::SecretsList | PaneId::SecretsDetail => ActiveTab::Secrets,
            PaneId::LLMsPolicyMatrix | PaneId::LLMsPolicySplit => ActiveTab::LLMs,
            PaneId::GitLedger => ActiveTab::Git,
            PaneId::JankSummary
            | PaneId::JankStatus
            | PaneId::JankScoreChart
            | PaneId::JankBreakdown
            | PaneId::JankIssues
            | PaneId::JankEntryDetail => ActiveTab::Jankurai,
        }
    }

    pub fn label(self) -> String {
        match self {
            PaneId::FleetBar => "Fleet".into(),
            PaneId::WorkflowMissionStrip => "Mission Strip".into(),
            PaneId::WorkflowPrRail => "PRs".into(),
            PaneId::WorkflowPhaseRail => "Phase".into(),
            PaneId::WorkflowCanvas => "Canvas".into(),
            PaneId::WorkflowMinimap => "Map".into(),
            PaneId::WorkflowInspector => "Inspector".into(),
            PaneId::ActivityLog(tab) => format!("Activity / Logs ({tab:?})"),
            PaneId::MissionTopSignal => "Top Signal".into(),
            PaneId::MissionReadiness => "Readiness".into(),
            PaneId::MissionMetrics => "Metrics".into(),
            PaneId::MissionAttention => "Attention".into(),
            PaneId::MissionProofLanes => "Proof Lanes".into(),
            PaneId::MissionActions => "Actions".into(),
            PaneId::ReleaseSelector => "Subpane Selector".into(),
            PaneId::ReleasePipeline => "Release".into(),
            PaneId::ReleaseInspector => "Inspector".into(),
            PaneId::ReleaseRollback => "Rollback".into(),
            PaneId::ApprovalsQueue => "Approvals".into(),
            PaneId::ApprovalsInspector => "Inspector".into(),
            PaneId::JobsRunnerFeed => "Runner Feed".into(),
            PaneId::JobsProgress => "Progress".into(),
            PaneId::JobsMatrix => "Job Matrix".into(),
            PaneId::JobsInspector => "Inspector".into(),
            PaneId::AgentsSessions => "Sessions".into(),
            PaneId::AgentsCockpit => "Cockpit".into(),
            PaneId::AgentsTimeline => "Timeline".into(),
            PaneId::AgentsActions => "Actions".into(),
            PaneId::TestsBottlenecks => "Bottlenecks".into(),
            PaneId::TestsHistory => "History".into(),
            PaneId::PoolsList => "Pools".into(),
            PaneId::PoolsDetail => "Detail".into(),
            PaneId::CacheDisk => "Disk".into(),
            PaneId::CacheStorage => "Storage".into(),
            PaneId::CacheGateway => "Gateway".into(),
            PaneId::CacheSingleflight => "Singleflight".into(),
            PaneId::CacheTaint => "Taint".into(),
            PaneId::EvidenceList => "Evidence".into(),
            PaneId::EvidenceDetail => "Detail".into(),
            PaneId::ReposLens => "Repos".into(),
            PaneId::BugsProjects => "Projects".into(),
            PaneId::BugsTable => "Bugs".into(),
            PaneId::BugsInspector => "Inspector".into(),
            PaneId::SecretsList => "Secrets".into(),
            PaneId::SecretsDetail => "Detail".into(),
            PaneId::LLMsPolicyMatrix => "Policy Matrix".into(),
            PaneId::LLMsPolicySplit => "Policy Split".into(),
            PaneId::GitLedger => "Ledger".into(),
            PaneId::JankSummary => "Jankurai Summary".into(),
            PaneId::JankStatus => "Jankurai Status".into(),
            PaneId::JankScoreChart => "Score History".into(),
            PaneId::JankBreakdown => "Last Scan Dimensions".into(),
            PaneId::JankIssues => "Caps / Findings".into(),
            PaneId::JankEntryDetail => "Entry Detail".into(),
        }
    }

    pub fn default_for_tab(tab: ActiveTab) -> Self {
        match tab {
            ActiveTab::Workflow => PaneId::WorkflowPrRail,
            ActiveTab::Mission => PaneId::MissionTopSignal,
            ActiveTab::Release => PaneId::ReleaseSelector,
            ActiveTab::Approvals => PaneId::ApprovalsQueue,
            ActiveTab::Jobs => PaneId::JobsRunnerFeed,
            ActiveTab::Agents => PaneId::AgentsSessions,
            ActiveTab::Tests => PaneId::TestsBottlenecks,
            ActiveTab::Pools => PaneId::PoolsList,
            ActiveTab::Cache => PaneId::CacheDisk,
            ActiveTab::Evidence => PaneId::EvidenceList,
            ActiveTab::Repos => PaneId::ReposLens,
            ActiveTab::Bugs => PaneId::BugsTable,
            ActiveTab::LLMs => PaneId::LLMsPolicyMatrix,
            ActiveTab::Git => PaneId::GitLedger,
            ActiveTab::Secrets => PaneId::SecretsList,
            ActiveTab::Jankurai => PaneId::JankIssues,
        }
    }

    pub fn panes_for_tab(tab: ActiveTab) -> &'static [PaneId] {
        use ActiveTab::*;
        match tab {
            Workflow => &[
                PaneId::WorkflowMissionStrip,
                PaneId::WorkflowPrRail,
                PaneId::WorkflowPhaseRail,
                PaneId::WorkflowCanvas,
                PaneId::WorkflowMinimap,
                PaneId::WorkflowInspector,
                PaneId::ActivityLog(Workflow),
            ],
            Mission => &[
                PaneId::MissionTopSignal,
                PaneId::MissionReadiness,
                PaneId::MissionMetrics,
                PaneId::MissionAttention,
                PaneId::MissionProofLanes,
                PaneId::MissionActions,
                PaneId::ActivityLog(Mission),
            ],
            Release => &[
                PaneId::ReleaseSelector,
                PaneId::ReleasePipeline,
                PaneId::ReleaseInspector,
                PaneId::ReleaseRollback,
                PaneId::ActivityLog(Release),
            ],
            Approvals => &[
                PaneId::ApprovalsQueue,
                PaneId::ApprovalsInspector,
                PaneId::ActivityLog(Approvals),
            ],
            Jobs => &[
                PaneId::JobsRunnerFeed,
                PaneId::JobsProgress,
                PaneId::JobsMatrix,
                PaneId::JobsInspector,
                PaneId::ActivityLog(Jobs),
            ],
            Agents => &[
                PaneId::AgentsSessions,
                PaneId::AgentsCockpit,
                PaneId::AgentsTimeline,
                PaneId::AgentsActions,
                PaneId::ActivityLog(Agents),
            ],
            Tests => &[
                PaneId::TestsBottlenecks,
                PaneId::TestsHistory,
                PaneId::ActivityLog(Tests),
            ],
            Pools => &[
                PaneId::PoolsList,
                PaneId::PoolsDetail,
                PaneId::ActivityLog(Pools),
            ],
            Cache => &[
                PaneId::CacheDisk,
                PaneId::CacheStorage,
                PaneId::CacheGateway,
                PaneId::CacheSingleflight,
                PaneId::CacheTaint,
                PaneId::ActivityLog(Cache),
            ],
            Evidence => &[
                PaneId::EvidenceList,
                PaneId::EvidenceDetail,
                PaneId::ActivityLog(Evidence),
            ],
            Repos => &[PaneId::ReposLens, PaneId::ActivityLog(Repos)],
            Bugs => &[
                PaneId::BugsProjects,
                PaneId::BugsTable,
                PaneId::BugsInspector,
                PaneId::ActivityLog(Bugs),
            ],
            Secrets => &[
                PaneId::SecretsList,
                PaneId::SecretsDetail,
                PaneId::ActivityLog(Secrets),
            ],
            LLMs => &[
                PaneId::LLMsPolicyMatrix,
                PaneId::LLMsPolicySplit,
                PaneId::ActivityLog(LLMs),
            ],
            Git => &[PaneId::GitLedger, PaneId::ActivityLog(Git)],
            Jankurai => &[
                PaneId::JankSummary,
                PaneId::JankStatus,
                PaneId::JankScoreChart,
                PaneId::JankBreakdown,
                PaneId::JankIssues,
                PaneId::JankEntryDetail,
            ],
        }
    }
}
