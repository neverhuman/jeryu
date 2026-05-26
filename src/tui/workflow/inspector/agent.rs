//! Owner: Interactive TUI subsystem — Inspector agent-call detail tab
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Render-only; reads app state, never mutates it.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::{draw_placeholder, empty_block, row};
use crate::tui::{
    theme::Theme,
    workflow::model::{WorkflowNode, WorkflowNodeKind},
};

pub(super) fn draw_agent(f: &mut Frame, area: Rect, node: Option<&WorkflowNode>, theme: &Theme) {
    let Some(node) = node else {
        return draw_placeholder(f, area, "no node selected", theme);
    };
    if !matches!(node.kind, WorkflowNodeKind::AgentReview { .. }) {
        return draw_placeholder(f, area, "not an agent-review node", theme);
    }
    let Some(detail) = node.agent_call.as_ref() else {
        return draw_placeholder(
            f,
            area,
            "agent call payload not yet attached — live receipt wiring pending",
            theme,
        );
    };
    let mut lines = Vec::new();
    lines.push(row(
        "Model",
        detail.model.as_deref().unwrap_or("(unknown)"),
        theme.bold(theme.agent),
        theme,
    ));
    if let Some(provider) = detail.provider.as_deref() {
        lines.push(row("Provider", provider, theme.secondary(), theme));
    }
    if let Some(agent_id) = detail.agent_id.as_deref() {
        lines.push(row("Agent", agent_id, theme.secondary(), theme));
    }
    let (decision_text, decision_style) = match detail.decision.as_deref() {
        Some("pass") => ("PASS", theme.bold(theme.ok)),
        Some("block") => ("BLOCK", theme.bold(theme.fail)),
        Some("concern") => ("CONCERN", theme.bold(theme.warning)),
        Some(other) => (other, theme.bold(theme.text_primary)),
        None => ("(pending)", theme.muted()),
    };
    lines.push(row("Decision", decision_text, decision_style, theme));
    if let Some(reason) = detail.reason.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Reason:",
            theme.bold(theme.text_primary),
        )));
        for chunk in reason.lines().take(8) {
            lines.push(Line::from(Span::styled(
                format!("    {}", chunk),
                theme.secondary(),
            )));
        }
    }
    if !detail.findings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Findings:",
            theme.bold(theme.text_primary),
        )));
        for finding in detail.findings.iter().take(8) {
            let color = match finding.severity.as_str() {
                "critical" => theme.fail,
                "high" => theme.fail,
                "medium" => theme.warning,
                "low" => theme.text_muted,
                _ => theme.text_muted,
            };
            let file = finding
                .file
                .as_deref()
                .map(|f| format!(" {}", f))
                .unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    [{}]", finding.severity),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}{}", finding.class, file), theme.secondary()),
            ]));
        }
    }
    if let Some(sha) = detail.raw_response_sha.as_deref() {
        lines.push(Line::from(""));
        lines.push(row(
            "Raw SHA",
            &sha[..sha.len().min(16)],
            theme.muted(),
            theme,
        ));
    }
    if let Some(json) = detail.decision_json.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Decision JSON:",
            theme.bold(theme.text_primary),
        )));
        for chunk in json.lines().take(16) {
            lines.push(Line::from(Span::styled(
                format!("    {}", chunk),
                theme.primary(),
            )));
        }
    }

    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(empty_block(theme, " Agent Call ")),
        area,
    );
}
