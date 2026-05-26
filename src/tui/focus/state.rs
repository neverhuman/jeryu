use crate::tui::app::ActiveTab;

use super::PaneId;

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
