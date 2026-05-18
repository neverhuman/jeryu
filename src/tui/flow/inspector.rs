//! Owner: Interactive TUI subsystem — flow inspector pane
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Inspector output is read-only and redacts sensitive trace material.

use super::model::FlowNode;
use crate::api::snapshot::{CacheVerdict, VtiStatus};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn draw_inspector(
    f: &mut Frame,
    area: Rect,
    node: Option<&FlowNode>,
    trace_tail: Option<&str>,
) {
    if let Some(n) = node {
        let title = format!(" JOB {} ", n.label);

        let color = match n.status.as_str() {
            "success" => Color::Green,
            "running" => Color::Blue,
            "failed" => Color::Red,
            "pending" | "created" | "waiting_for_resource" | "preparing" => Color::Yellow,
            "canceled" => Color::DarkGray,
            _ => Color::Gray,
        };

        let eta_str = if let Some(ref e) = n.eta {
            format!("{}s", e.remaining_secs)
        } else {
            "N/A".to_string()
        };

        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("  Status: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &n.status,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}%", n.progress_pct),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("  ETA:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(&eta_str, Style::default().fg(Color::White)),
                Span::styled(
                    format!("  Elapsed: {}s", n.elapsed_secs),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Phase:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:?}", n.column), Style::default().fg(Color::Cyan)),
                Span::styled("  Lane: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:?}", n.lane), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("  Flags:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if n.is_required { "required " } else { "" },
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    if n.is_critical_path { "[CRIT] " } else { "" },
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        // VTI status line
        if let Some(ref vti) = n.vti_status {
            let (vti_label, vti_color) = match vti {
                VtiStatus::Accelerated {
                    reason,
                    time_saved_secs,
                } => (
                    format!("🔥 Accelerated — saved {}s — {}", time_saved_secs, reason),
                    Color::Rgb(255, 165, 0),
                ),
                VtiStatus::Skipped { reason, confidence } => (
                    format!("⊘ Skipped (conf {:.0}%) — {}", confidence * 100.0, reason),
                    Color::DarkGray,
                ),
                VtiStatus::Selected { reason, confidence } => (
                    format!("✓ Selected (conf {:.0}%) — {}", confidence * 100.0, reason),
                    Color::Green,
                ),
                VtiStatus::FullSuite => ("Full suite (no VTI filtering)".to_string(), Color::Gray),
            };
            lines.push(Line::from(vec![
                Span::styled("  VTI:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    vti_label,
                    Style::default().fg(vti_color).add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        // Cache verdict line
        if let Some(ref cache) = n.cache_verdict {
            let (cache_label, cache_color) = match cache {
                CacheVerdict::Hit { trust } => (format!("HIT (trust: {:?})", trust), Color::Green),
                CacheVerdict::Miss => ("MISS".to_string(), Color::Yellow),
                CacheVerdict::Tainted { reason } => {
                    (format!("TAINTED — {}", reason), Color::Magenta)
                }
                CacheVerdict::Denied { reason } => (format!("DENIED — {}", reason), Color::Red),
            };
            lines.push(Line::from(vec![
                Span::styled("  Cache:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(cache_label, Style::default().fg(cache_color)),
            ]));
        }

        // Flake probability
        if let Some(flake) = n.flake_probability
            && flake > 0.01
        {
            let flake_color = if flake > 0.15 {
                Color::Red
            } else if flake > 0.05 {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            lines.push(Line::from(vec![
                Span::styled("  Flake:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.1}%", flake * 100.0),
                    Style::default().fg(flake_color),
                ),
            ]));
        }

        // Agent badge
        if let Some(ref agent_id) = n.agent_id {
            lines.push(Line::from(vec![
                Span::styled("  Agent:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    agent_id.clone(),
                    Style::default().fg(Color::Rgb(102, 255, 255)),
                ),
            ]));
        }

        // Capsule badge
        if let Some(ref capsule_id) = n.capsule_id {
            lines.push(Line::from(vec![
                Span::styled("  Capsule:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", capsule_id),
                    Style::default().fg(Color::Yellow),
                ),
            ]));
        }

        // Retry lineage
        if !n.attempt_lineage.is_empty() {
            let lineage_str: String = n
                .attempt_lineage
                .iter()
                .map(|id| format!("#{}", id))
                .collect::<Vec<_>>()
                .join(" → ");
            lines.push(Line::from(vec![
                Span::styled("  Retries:", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {}", lineage_str),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        // Trace tail
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Trace tail:",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        let tail = trace_tail.unwrap_or("Waiting for logs...");
        for line in tail.lines().take(6) {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Color::White),
            )));
        }

        let p = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(p, area);
    } else {
        let p =
            Paragraph::new("No job selected or graph is empty.\nUse arrow keys to navigate flow.")
                .block(
                    Block::default()
                        .title(" [ Inspector ] ")
                        .borders(Borders::ALL),
                );
        f.render_widget(p, area);
    }
}
