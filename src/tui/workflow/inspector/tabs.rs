//! Owner: Interactive TUI subsystem — Inspector tab enum + strip
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Render-only; reads app state, never mutates it.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::{
    focus::PaneChrome,
    theme::Theme,
    workflow::model::{PullRequestView, WorkflowNode, WorkflowNodeKind},
};

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

    /// Returns true when this tab is visible for the given node. The
    /// `Agent` tab is hidden when the node isn't an `AgentReview`.
    pub fn visible_for(self, node: Option<&WorkflowNode>) -> bool {
        if self != Self::Agent {
            return true;
        }
        matches!(
            node.map(|n| &n.kind),
            Some(WorkflowNodeKind::AgentReview { .. })
        )
    }

    /// Cycle forward, skipping tabs that aren't visible for `node`.
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

    /// Cycle backward, skipping tabs that aren't visible for `node`.
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

pub(super) fn draw_tab_strip(
    f: &mut Frame,
    area: Rect,
    pr: &PullRequestView,
    node: Option<&WorkflowNode>,
    selected: InspectorTab,
    theme: &Theme,
    chrome: Option<PaneChrome>,
) {
    let selected_text = match node {
        Some(n) => format!(
            "{} {}",
            n.status.glyph(),
            n.label.chars().take(28).collect::<String>()
        ),
        None => format!("PR #{}", pr.number),
    };
    let title_text = format!("Inspector · {selected_text}");
    let title = match chrome {
        Some(chrome) => chrome.title(&title_text),
        None => crate::tui::focus::title_with_esc(&title_text, false),
    };
    let border_style = match chrome {
        Some(chrome) => chrome.border_style,
        None => Style::default().fg(theme.border_accent),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans: Vec<Span> = Vec::new();
    for tab in InspectorTab::ALL {
        if !tab.visible_for(node) {
            continue;
        }
        let style = if tab == selected {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.border_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.muted()
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}
