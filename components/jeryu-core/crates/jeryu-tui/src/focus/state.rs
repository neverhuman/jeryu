//! Focus state stack (faithful port of the source focus state machine).
//!
//! Invariants: [`FocusState::active`] always points at a valid [`PaneId`] for
//! the current tab; `stack` records the drill-down breadcrumb (most-recent
//! last); `fullscreen` is `Some` only while a single pane occupies the whole
//! content area. Tab change resets the stack and clears fullscreen.

use super::pane::PaneId;
use crate::app::ActiveTab;

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

    /// Cycle focus to the next pane within `tab` (wraps).
    pub fn focus_next(&mut self, tab: ActiveTab) {
        let panes = PaneId::panes_for_tab(tab);
        if panes.is_empty() {
            return;
        }
        let idx = panes.iter().position(|p| *p == self.active).unwrap_or(0);
        self.active = panes[(idx + 1) % panes.len()];
    }

    /// Cycle focus to the previous pane within `tab` (wraps).
    pub fn focus_prev(&mut self, tab: ActiveTab) {
        let panes = PaneId::panes_for_tab(tab);
        if panes.is_empty() {
            return;
        }
        let idx = panes.iter().position(|p| *p == self.active).unwrap_or(0);
        self.active = panes[(idx + panes.len() - 1) % panes.len()];
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

    /// Enter fullscreen on the currently-active pane.
    pub fn enter_fullscreen(&mut self) {
        self.fullscreen = Some(self.active);
    }

    pub fn escape(&mut self) -> bool {
        if self.fullscreen.take().is_some() {
            return self.pop();
        }
        self.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_tab_selects_default_pane() {
        let fs = FocusState::for_tab(ActiveTab::Mission);
        assert_eq!(fs.active, PaneId::MissionTopSignal);
        assert!(!fs.is_drilled());
    }

    #[test]
    fn set_tab_clears_stack_and_fullscreen() {
        let mut fs = FocusState::for_tab(ActiveTab::Workflow);
        fs.push();
        fs.enter_fullscreen();
        assert!(fs.is_drilled());
        fs.set_tab(ActiveTab::Repos);
        assert_eq!(fs.active, PaneId::ReposLens);
        assert!(!fs.is_drilled());
    }

    #[test]
    fn push_pop_restores_previous_pane() {
        let mut fs = FocusState::for_tab(ActiveTab::Mission);
        let first = fs.active;
        fs.push();
        fs.active = PaneId::MissionAttention;
        assert!(fs.pop());
        assert_eq!(fs.active, first);
        assert!(!fs.pop());
    }

    #[test]
    fn focus_next_and_prev_wrap_within_tab() {
        let mut fs = FocusState::for_tab(ActiveTab::Mission);
        let panes = PaneId::panes_for_tab(ActiveTab::Mission);
        // active starts at default (MissionTopSignal) which is panes[0].
        fs.active = panes[0];
        fs.focus_prev(ActiveTab::Mission);
        assert_eq!(fs.active, *panes.last().unwrap());
        fs.focus_next(ActiveTab::Mission);
        assert_eq!(fs.active, panes[0]);
    }

    #[test]
    fn escape_unwinds_fullscreen_then_stack() {
        let mut fs = FocusState::for_tab(ActiveTab::Mission);
        fs.push();
        fs.active = PaneId::MissionMetrics;
        fs.enter_fullscreen();
        // escape pops fullscreen + one stack frame
        assert!(fs.escape());
        assert!(fs.fullscreen.is_none());
        assert_eq!(fs.active, PaneId::MissionTopSignal);
    }
}
