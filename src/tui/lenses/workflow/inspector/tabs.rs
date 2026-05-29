use crate::tui::lenses::workflow::model::{WorkflowNode, WorkflowNodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Overview,
    Agent,
    Logs,
    Deps,
    Evidence,
    Actions,
}

impl InspectorTab {
    pub const ALL: [InspectorTab; 6] = [
        Self::Overview,
        Self::Agent,
        Self::Logs,
        Self::Deps,
        Self::Evidence,
        Self::Actions,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Agent => "Agent",
            Self::Logs => "Logs",
            Self::Deps => "Deps",
            Self::Evidence => "Evidence",
            Self::Actions => "Actions",
        }
    }

    pub fn visible_for(self, node: Option<&WorkflowNode>) -> bool {
        if self != Self::Agent {
            return true;
        }
        matches!(
            node.map(|n| &n.kind),
            Some(WorkflowNodeKind::AgentReview { .. })
        )
    }

    pub fn next_for(self, node: Option<&WorkflowNode>) -> Self {
        let n = Self::ALL.len();
        let start = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        for offset in 1..=n {
            let idx = (start + offset) % n;
            if Self::ALL[idx].visible_for(node) {
                return Self::ALL[idx];
            }
        }
        self
    }

    pub fn prev_for(self, node: Option<&WorkflowNode>) -> Self {
        let n = Self::ALL.len();
        let start = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        for offset in 1..=n {
            let idx = (start + n - offset) % n;
            if Self::ALL[idx].visible_for(node) {
                return Self::ALL[idx];
            }
        }
        self
    }

    pub fn next(self) -> Self {
        self.next_for(None)
    }

    pub fn prev(self) -> Self {
        self.prev_for(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::lenses::workflow::model::{AgentStage, WorkflowNode};

    #[test]
    fn tab_cycles_next_and_prev_without_agent() {
        let visible = InspectorTab::ALL
            .iter()
            .filter(|t| t.visible_for(None))
            .count();
        assert_eq!(visible, 5);

        let mut t = InspectorTab::Overview;
        for _ in 0..visible {
            t = t.next();
        }
        assert_eq!(t, InspectorTab::Overview);

        let mut t = InspectorTab::Logs;
        t = t.prev();
        assert_eq!(t, InspectorTab::Overview);
    }

    #[test]
    fn agent_tab_visible_only_for_agent_review_nodes() {
        let agent_node = WorkflowNode {
            kind: WorkflowNodeKind::AgentReview {
                stage: AgentStage::PreMerge,
            },
            ..Default::default()
        };
        let plain_node = WorkflowNode {
            kind: WorkflowNodeKind::UnitTest,
            ..Default::default()
        };
        assert!(InspectorTab::Agent.visible_for(Some(&agent_node)));
        assert!(!InspectorTab::Agent.visible_for(Some(&plain_node)));
        assert!(!InspectorTab::Agent.visible_for(None));
    }

    #[test]
    fn next_for_includes_agent_on_agent_node() {
        let agent_node = WorkflowNode {
            kind: WorkflowNodeKind::AgentReview {
                stage: AgentStage::PreMerge,
            },
            ..Default::default()
        };
        let t = InspectorTab::Overview.next_for(Some(&agent_node));
        assert_eq!(t, InspectorTab::Agent);
    }
}
