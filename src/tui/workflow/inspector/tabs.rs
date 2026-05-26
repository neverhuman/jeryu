//! Owner: Interactive TUI subsystem — Delivery inspector internal tab strip
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Render-only; `InspectorTab` is a public enum used by callers
//! to remember the selected sub-pane.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::model::*;
use crate::tui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Overview,
    Logs,
    Deps,
    Evidence,
    Actions,
}

impl InspectorTab {
    pub const ALL: [InspectorTab; 5] = [
        Self::Overview,
        Self::Logs,
        Self::Deps,
        Self::Evidence,
        Self::Actions,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Logs => "Logs",
            Self::Deps => "Deps",
            Self::Evidence => "Evidence",
            Self::Actions => "Actions",
        }
    }

    pub fn next(self) -> Self {
        let mut idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        idx = (idx + 1) % Self::ALL.len();
        Self::ALL[idx]
    }

    pub fn prev(self) -> Self {
        let mut idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        idx = if idx == 0 {
            Self::ALL.len() - 1
        } else {
            idx - 1
        };
        Self::ALL[idx]
    }
}

pub(super) fn draw_tab_strip(
    f: &mut Frame,
    area: Rect,
    pr: &PullRequestView,
    node: Option<&WorkflowNode>,
    selected: InspectorTab,
    theme: &Theme,
) {
    let title_text = match node {
        Some(n) => format!(
            " {} {} ",
            n.status.glyph(),
            n.label.chars().take(28).collect::<String>()
        ),
        None => format!(" PR #{} ", pr.number),
    };
    let block = Block::default()
        .title(title_text)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_accent));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut spans: Vec<Span> = Vec::new();
    for tab in InspectorTab::ALL {
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
