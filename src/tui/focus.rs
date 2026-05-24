use crate::tui::app::{ActiveTab, App};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
};

#[path = "focus_map.rs"]
mod focus_map;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
    /// Global fleet bar — visible on every tab, above the content area.
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
    BugsProjects,
    BugsTable,
    BugsInspector,
    SecretsList,
    SecretsDetail,
    LLMsPolicyMatrix,
    LLMsPolicySplit,
    GitLedger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct PaneChrome {
    pub border_style: Style,
    pub show_esc: bool,
}

impl PaneChrome {
    pub fn title(self, label: &str) -> String {
        title_with_esc(label, self.show_esc)
    }
}

#[derive(Debug, Clone)]
pub struct FocusPane {
    pub id: PaneId,
    pub rect: Rect,
}

#[derive(Debug, Clone)]
pub struct FocusMap {
    pub tab: ActiveTab,
    pub panes: Vec<FocusPane>,
    pub esc_targets: Vec<(PaneId, Rect)>,
}

impl Default for FocusMap {
    fn default() -> Self {
        Self {
            tab: ActiveTab::Workflow,
            panes: Vec::new(),
            esc_targets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FocusState {
    pub active: PaneId,
    pub stack: Vec<PaneId>,
    pub fullscreen: Option<PaneId>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self::for_tab(ActiveTab::Workflow)
    }
}

impl FocusState {
    pub fn for_tab(tab: ActiveTab) -> Self {
        Self {
            active: PaneId::default_for_tab(tab),
            stack: Vec::new(),
            fullscreen: None,
        }
    }

    pub fn set_tab(&mut self, tab: ActiveTab) {
        self.active = PaneId::default_for_tab(tab);
        self.stack.clear();
        self.fullscreen = None;
    }

    pub fn is_drilled(&self) -> bool {
        self.fullscreen.is_some() || !self.stack.is_empty()
    }

    pub fn push(&mut self) {
        self.stack.push(self.active);
    }

    pub fn pop(&mut self) -> bool {
        if let Some(prev) = self.stack.pop() {
            self.active = prev;
            true
        } else {
            false
        }
    }

    pub fn escape(&mut self) -> bool {
        if self.fullscreen.take().is_some() {
            return self.pop();
        }
        self.pop()
    }
}

impl PaneId {
    /// Returns `true` for panes that appear on every tab (e.g. FleetBar).
    pub fn is_global(self) -> bool {
        matches!(self, PaneId::FleetBar)
    }

    pub fn tab(self) -> ActiveTab {
        match self {
            // FleetBar is global — report as Workflow as a fallback; callers
            // should check `is_global()` first when the mapping matters.
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
            PaneId::BugsProjects | PaneId::BugsTable | PaneId::BugsInspector => ActiveTab::Bugs,
            PaneId::SecretsList | PaneId::SecretsDetail => ActiveTab::Secrets,
            PaneId::LLMsPolicyMatrix | PaneId::LLMsPolicySplit => ActiveTab::LLMs,
            PaneId::GitLedger => ActiveTab::Git,
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
            PaneId::BugsProjects => "Projects".into(),
            PaneId::BugsTable => "Bugs".into(),
            PaneId::BugsInspector => "Inspector".into(),
            PaneId::SecretsList => "Secrets".into(),
            PaneId::SecretsDetail => "Detail".into(),
            PaneId::LLMsPolicyMatrix => "Policy Matrix".into(),
            PaneId::LLMsPolicySplit => "Policy Split".into(),
            PaneId::GitLedger => "Ledger".into(),
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
            ActiveTab::Bugs => PaneId::BugsTable,
            ActiveTab::LLMs => PaneId::LLMsPolicyMatrix,
            ActiveTab::Git => PaneId::GitLedger,
            ActiveTab::Secrets => PaneId::SecretsList,
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
        }
    }
}

pub fn border_color(app: &App, pane: PaneId) -> Color {
    let is_maximized_active_log =
        app.maximize_logs && matches!(pane, PaneId::ActivityLog(tab) if tab == app.active_tab);

    if app.focus.fullscreen == Some(pane) || is_maximized_active_log {
        Color::Cyan
    } else if app.focus.active == pane {
        Color::Yellow
    } else if app.focus.stack.last().copied() == Some(pane) {
        Color::Magenta
    } else {
        Color::DarkGray
    }
}

pub fn border_style(app: &App, pane: PaneId) -> Style {
    let mut style = Style::default().fg(border_color(app, pane));
    if app.focus.active == pane || app.focus.fullscreen == Some(pane) {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

pub fn is_active(app: &App, pane: PaneId) -> bool {
    app.focus.active == pane
        || app.focus.fullscreen == Some(pane)
        || (app.maximize_logs && matches!(pane, PaneId::ActivityLog(tab) if tab == app.active_tab))
}

pub fn should_show_esc(app: &App, pane: PaneId) -> bool {
    is_active(app, pane) || app.focus.stack.last().copied() == Some(pane)
}

pub fn should_show_drill_esc(app: &App, pane: PaneId) -> bool {
    app.focus.is_drilled() && should_show_esc(app, pane)
}

pub fn pane_chrome(app: &App, pane: PaneId) -> PaneChrome {
    PaneChrome {
        border_style: border_style(app, pane),
        show_esc: should_show_drill_esc(app, pane),
    }
}

pub fn register_pane(app: &mut App, pane: PaneId, rect: Rect) {
    app.focus_map.register(pane, rect);
}

pub fn register_esc_hotspot(app: &mut App, pane: PaneId, rect: Rect) {
    if should_show_esc(app, pane) {
        // Make the full title row clickable so the close affordance is not
        // sensitive to title placement or terminal width differences.
        if rect.width > 0 {
            let esc = Rect::new(rect.x, rect.y, rect.width, 1);
            app.focus_map.register_esc(pane, esc);
        }
    }
}

pub fn register_drill_esc_hotspot(app: &mut App, pane: PaneId, rect: Rect) {
    if should_show_drill_esc(app, pane) && rect.width > 0 {
        let esc = Rect::new(rect.x, rect.y, rect.width, 1);
        app.focus_map.register_esc(pane, esc);
    }
}

pub fn esc_label(active: bool) -> &'static str {
    if active { " [esc] " } else { "" }
}

pub fn title_with_esc(label: &str, show_esc: bool) -> String {
    format!(" {label}{} ", esc_label(show_esc))
}
