//! Application state spine: the active-tab enum + the render-only `App` state
//! that the lenses project from.
//!
//! Faithful port of the source TUI app spine, minus the source-control /
//! sandbox data plane. The data spine here is a single immutable
//! [`TuiReadModel`] (from the
//! `jeryu-readmodel` contract) plus the [`FocusState`] machine. A future core
//! fills the read model behind the [`ReadModelSource`](jeryu_readmodel::seam)
//! seam; the TUI only ever consumes the contract.

use jeryu_readmodel::TuiReadModel;

use crate::focus::FocusState;
use crate::lenses::agents::AgentTerminalSession;
use crate::runtime::session::SessionHandle;

/// Phase of an operator-initiated agent session launch (the Agents lens `n`
/// affordance). The lens marks a launch [`Pending`](SessionLaunchPhase::Pending)
/// the instant `n` is pressed, then [`Attached`](SessionLaunchPhase::Attached)
/// once the control plane returns the new run's [`SessionHandle`] and its live
/// terminal is mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLaunchPhase {
    /// The create-session request is in flight.
    Pending,
    /// The session was created and its live terminal is attached.
    Attached,
}

/// State of the most recent "New Session" launch, surfaced in the Agents lens
/// banner. Absent by default, so a render with no launch in progress is
/// byte-identical to the pre-affordance frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLaunch {
    /// The repository the session was launched on.
    pub repo_id: String,
    /// Where the launch is in its lifecycle.
    pub phase: SessionLaunchPhase,
    /// The created run id, once the control plane has answered.
    pub run_id: Option<String>,
    /// The isolated working branch, once the control plane has answered.
    pub branch: Option<String>,
}

impl SessionLaunch {
    /// A launch whose create-session request is still in flight.
    pub fn pending(repo_id: impl Into<String>) -> Self {
        Self {
            repo_id: repo_id.into(),
            phase: SessionLaunchPhase::Pending,
            run_id: None,
            branch: None,
        }
    }

    /// Advance the launch to attached, recording the run/branch the control
    /// plane returned.
    pub fn attach(repo_id: impl Into<String>, handle: &SessionHandle) -> Self {
        Self {
            repo_id: repo_id.into(),
            phase: SessionLaunchPhase::Attached,
            run_id: Some(handle.run_id.clone()),
            branch: Some(handle.branch.clone()),
        }
    }

    /// Whether the create-session request is still in flight.
    pub fn is_pending(&self) -> bool {
        self.phase == SessionLaunchPhase::Pending
    }

    /// Short status word for the affordance banner.
    pub fn status_label(&self) -> &'static str {
        match self.phase {
            SessionLaunchPhase::Pending => "launching",
            SessionLaunchPhase::Attached => "attached",
        }
    }
}

/// The Flight Deck tab set. Each tab routes to exactly one lens; the digit
/// shortcuts (`from_number`) and `Tab`/`BackTab` cycling are preserved from the
/// source product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ActiveTab {
    #[default]
    Workflow,
    Mission,
    Release,
    Approvals,
    Jobs,
    Agents,
    Tests,
    Pools,
    /// Operator Pools/Health (runner-pool fleet) tab. Routes to the Runners
    /// lens, which projects [`PoolActivity`](jeryu_readmodel::PoolActivity).
    Runners,
    Cache,
    Evidence,
    Repos,
    Bugs,
    LLMs,
    Git,
    Secrets,
    /// Jankurai audit overview. Reached via Tab / BackTab cycling — no digit
    /// shortcut so the 0-9 layout stays stable.
    Jankurai,
}

impl ActiveTab {
    /// All tabs in stable header order.
    pub const ALL: &'static [ActiveTab] = &[
        Self::Workflow,
        Self::Mission,
        Self::Release,
        Self::Approvals,
        Self::Jobs,
        Self::Agents,
        Self::Tests,
        Self::Pools,
        Self::Runners,
        Self::Cache,
        Self::Evidence,
        Self::Repos,
        Self::Bugs,
        Self::LLMs,
        Self::Git,
        Self::Secrets,
        Self::Jankurai,
    ];

    /// Digit shortcut mapping (0-9). Preserved verbatim from the source.
    pub fn from_number(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Workflow),
            1 => Some(Self::Mission),
            2 => Some(Self::Release),
            3 => Some(Self::Approvals),
            4 => Some(Self::Jobs),
            5 => Some(Self::Agents),
            6 => Some(Self::Tests),
            7 => Some(Self::Pools),
            8 => Some(Self::Cache),
            9 => Some(Self::Evidence),
            _ => None,
        }
    }

    /// Short header label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Workflow => "Workflow",
            Self::Mission => "Mission",
            Self::Release => "Release",
            Self::Approvals => "Approvals",
            Self::Jobs => "Jobs",
            Self::Agents => "Agents",
            Self::Tests => "Tests",
            Self::Pools => "Pools",
            Self::Runners => "Runners",
            Self::Cache => "Cache",
            Self::Evidence => "Evidence",
            Self::Repos => "Repos",
            Self::Bugs => "Bugs",
            Self::LLMs => "LLMs",
            Self::Git => "Git",
            Self::Secrets => "Secrets",
            Self::Jankurai => "Jankurai",
        }
    }

    /// Index of this tab within [`ActiveTab::ALL`] (for the header tab strip).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    /// Next tab in cycle order (wraps).
    pub fn next(self) -> Self {
        let i = self.index();
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    /// Previous tab in cycle order (wraps).
    pub fn prev(self) -> Self {
        let i = self.index();
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Render-only application state. Holds the immutable read-model snapshot the
/// lenses project from, the current tab, and the focus state machine.
///
/// This is the seam where the source-control / sandbox data spine of the source
/// product has been replaced by a single [`TuiReadModel`] fed from the contract
/// crate.
#[derive(Debug, Clone)]
pub struct App {
    pub model: TuiReadModel,
    pub active_tab: ActiveTab,
    pub focus: FocusState,
    /// The live agent-terminal session, when one has been opened on the Agents
    /// tab. `None` for every other surface, so non-Agents snapshots are
    /// unaffected by this pane.
    pub terminal: Option<AgentTerminalSession>,
    /// The most recent "New Session" launch, when the operator has pressed `n`
    /// on the Agents tab. `None` by default, so every other frame is unchanged
    /// by this affordance.
    pub session_launch: Option<SessionLaunch>,
}

impl App {
    /// Build a render-only app from an immutable read-model snapshot.
    pub fn new_render_only(model: TuiReadModel) -> Self {
        let active_tab = ActiveTab::default();
        Self {
            model,
            active_tab,
            focus: FocusState::for_tab(active_tab),
            terminal: None,
            session_launch: None,
        }
    }

    /// Switch the active tab and reset the focus stack (mirrors the source).
    pub fn set_tab(&mut self, tab: ActiveTab) {
        self.active_tab = tab;
        self.focus.set_tab(tab);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new_render_only(TuiReadModel::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_number_matches_source_layout() {
        assert_eq!(ActiveTab::from_number(0), Some(ActiveTab::Workflow));
        assert_eq!(ActiveTab::from_number(1), Some(ActiveTab::Mission));
        assert_eq!(ActiveTab::from_number(9), Some(ActiveTab::Evidence));
        assert_eq!(ActiveTab::from_number(10), None);
    }

    #[test]
    fn tab_cycle_wraps() {
        assert_eq!(ActiveTab::Workflow.prev(), ActiveTab::Jankurai);
        assert_eq!(ActiveTab::Jankurai.next(), ActiveTab::Workflow);
    }

    #[test]
    fn set_tab_resets_focus() {
        let mut app = App::default();
        app.focus.push();
        assert!(app.focus.is_drilled());
        app.set_tab(ActiveTab::Mission);
        assert_eq!(app.active_tab, ActiveTab::Mission);
        assert!(!app.focus.is_drilled());
    }
}
