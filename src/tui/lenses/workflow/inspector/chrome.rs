use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::InspectorTab;
use crate::tui::{
    focus::{PaneChrome, title_with_esc},
    lenses::workflow::model::{PullRequestView, WorkflowNode},
    theme::Theme,
};

pub(super) fn empty_block<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle))
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
        None => title_with_esc(&title_text, false),
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

pub(super) fn draw_placeholder(f: &mut Frame, area: Rect, msg: &str, theme: &Theme) {
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {}", msg), theme.muted())),
        ])
        .block(empty_block(theme, "")),
        area,
    );
}

pub(super) fn row<'a>(label: &str, value: &str, value_style: Style, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {:<10}", label), theme.muted()),
        Span::styled(value.to_string(), value_style),
    ])
}
