use crate::tui::app::{ActiveTab, App};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
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
    pub fn tab(self) -> ActiveTab {
        match self {
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
            PaneId::SecretsList | PaneId::SecretsDetail => ActiveTab::Secrets,
            PaneId::LLMsPolicyMatrix | PaneId::LLMsPolicySplit => ActiveTab::LLMs,
            PaneId::GitLedger => ActiveTab::Git,
        }
    }

    pub fn label(self) -> String {
        match self {
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

impl FocusMap {
    pub fn clear_for_tab(&mut self, tab: ActiveTab) {
        self.tab = tab;
        self.panes.clear();
        self.esc_targets.clear();
    }

    pub fn register(&mut self, id: PaneId, rect: Rect) {
        if rect.width > 0 && rect.height > 0 {
            self.panes.push(FocusPane { id, rect });
        }
    }

    pub fn register_esc(&mut self, id: PaneId, rect: Rect) {
        if rect.width > 0 && rect.height > 0 {
            self.esc_targets.push((id, rect));
        }
    }

    pub fn pane_at(&self, x: u16, y: u16) -> Option<PaneId> {
        self.panes
            .iter()
            .rev()
            .find(|pane| rect_contains(pane.rect, x, y))
            .map(|pane| pane.id)
    }

    pub fn esc_at(&self, x: u16, y: u16) -> Option<PaneId> {
        self.esc_targets
            .iter()
            .rev()
            .find(|(_, rect)| rect_contains(*rect, x, y))
            .map(|(id, _)| *id)
    }

    pub fn rect_of(&self, id: PaneId) -> Option<Rect> {
        self.panes
            .iter()
            .rev()
            .find(|pane| pane.id == id)
            .map(|pane| pane.rect)
    }

    pub fn first_visible(&self) -> Option<PaneId> {
        self.panes.first().map(|pane| pane.id)
    }

    pub fn neighbor(&self, from: PaneId, direction: NavDirection) -> Option<PaneId> {
        let origin = self.rect_of(from)?;
        let ox = center_x(origin);
        let oy = center_y(origin);

        self.panes
            .iter()
            .filter(|pane| pane.id != from)
            .filter_map(|pane| {
                let cx = center_x(pane.rect);
                let cy = center_y(pane.rect);
                let directional_ok = match direction {
                    NavDirection::Left => cx < ox,
                    NavDirection::Right => cx > ox,
                    NavDirection::Up => cy < oy,
                    NavDirection::Down => cy > oy,
                };
                if !directional_ok {
                    return None;
                }
                let dx = (cx as i32 - ox as i32).abs();
                let dy = (cy as i32 - oy as i32).abs();
                let score = match direction {
                    NavDirection::Left | NavDirection::Right => dx * 4 + dy,
                    NavDirection::Up | NavDirection::Down => dy * 4 + dx,
                };
                Some((score, pane.id))
            })
            .min_by_key(|(score, _)| *score)
            .map(|(_, id)| id)
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
    app.focus.fullscreen == Some(pane) || app.focus.stack.last().copied() == Some(pane)
}

pub fn register_pane(app: &mut App, pane: PaneId, rect: Rect) {
    app.focus_map.register(pane, rect);
}

pub fn register_focus_pane(app: &mut App, pane: PaneId, rect: Rect) {
    register_pane(app, pane, rect);
    register_esc_hotspot(app, pane, rect);
}

pub fn register_esc_hotspot(app: &mut App, pane: PaneId, rect: Rect) {
    if should_show_esc(app, pane) {
        let width = rect.width.saturating_sub(2).min(24);
        if width > 0 {
            let esc = Rect::new(rect.x + 1, rect.y, width, 1);
            app.focus_map.register_esc(pane, esc);
        }
    }
}

pub fn esc_label(active: bool) -> &'static str {
    if active { " [esc] " } else { "" }
}

pub fn pane_block<'a, T>(app: &App, pane: PaneId, title: T) -> Block<'a>
where
    T: Into<Line<'a>>,
{
    let mut title_line: Line<'a> = title.into();
    if should_show_esc(app, pane) {
        title_line.spans.push(Span::raw(" [esc]"));
    }
    Block::default()
        .title(title_line)
        .borders(Borders::ALL)
        .border_style(border_style(app, pane))
}

fn center_x(rect: Rect) -> u16 {
    rect.x + rect.width.saturating_div(2)
}

fn center_y(rect: Rect) -> u16 {
    rect.y + rect.height.saturating_div(2)
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panes_for_tab_have_stable_defaults_and_neighbor_links() {
        let tabs = [
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
            ActiveTab::Secrets,
            ActiveTab::LLMs,
            ActiveTab::Git,
        ];

        for tab in tabs {
            let panes = PaneId::panes_for_tab(tab);
            assert!(
                !panes.is_empty(),
                "every tab should expose at least one focusable pane"
            );
            assert_eq!(
                PaneId::default_for_tab(tab).tab(),
                tab,
                "default pane should belong to the tab it activates"
            );
            assert!(
                panes.contains(&PaneId::default_for_tab(tab)),
                "default pane should be present in the pane list for {tab:?}"
            );

            let mut map = FocusMap::default();
            map.clear_for_tab(tab);
            for (idx, pane) in panes.iter().copied().enumerate() {
                map.register(pane, Rect::new(4 + idx as u16 * 12, 2, 10, 4));
            }

            for window in panes.windows(2) {
                assert_eq!(
                    map.neighbor(window[0], NavDirection::Right),
                    Some(window[1]),
                    "right arrow should move from {:?} to {:?} on {tab:?}",
                    window[0],
                    window[1]
                );
                assert_eq!(
                    map.neighbor(window[1], NavDirection::Left),
                    Some(window[0]),
                    "left arrow should move from {:?} to {:?} on {tab:?}",
                    window[1],
                    window[0]
                );
            }
        }
    }
}
