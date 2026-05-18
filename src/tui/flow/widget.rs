//! Owner: Interactive TUI subsystem — flow graph widget
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: Widget rendering is pure over the supplied graph and selected node.

use super::model::{FlowColumnKind, FlowGraph};
use crate::api::snapshot::{CacheVerdict, EdgeKind, VtiStatus};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

pub struct FlowGraphWidget<'a> {
    pub graph: &'a FlowGraph,
    pub selected_node_id: Option<i64>,
}

impl<'a> FlowGraphWidget<'a> {
    pub fn new(graph: &'a FlowGraph, selected_node_id: Option<i64>) -> Self {
        Self {
            graph,
            selected_node_id,
        }
    }
}

impl<'a> Widget for FlowGraphWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.graph.columns.is_empty() {
            buf.set_string(area.x, area.y, "Waiting for job graph...", Style::default());
            return;
        }

        let total_cols = self.graph.columns.len() as u16;
        let col_width = if total_cols > 0 {
            area.width / total_cols
        } else {
            area.width
        };

        // Headers
        for (i, col) in self.graph.columns.iter().enumerate() {
            let x = area.x + (i as u16 * col_width);
            if x < area.right() {
                buf.set_stringn(
                    x,
                    area.y,
                    &col.title,
                    col_width as usize,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                );
            }
        }

        // Separator line
        let y_sep = area.y + 1;
        if y_sep < area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y_sep)]
                    .set_symbol("─")
                    .set_style(Style::default().fg(Color::DarkGray));
            }
        }

        // Build a map of node_id -> (col_index, y_position) for edge rendering
        let mut node_positions: std::collections::HashMap<i64, (u16, u16)> =
            std::collections::HashMap::new();

        // Draw nodes by lane groups
        let mut max_y_drawn = y_sep + 1;

        for (i, col) in self.graph.columns.iter().enumerate() {
            let mut y_cur = y_sep + 1;
            let x_base = area.x + (i as u16 * col_width);

            for group in &col.lane_groups {
                if y_cur >= area.bottom() {
                    break;
                }

                // Print lane header if it's Tests/Security
                if col.key == FlowColumnKind::Tests || col.key == FlowColumnKind::Security {
                    buf.set_stringn(
                        x_base,
                        y_cur,
                        format!("├─ {}", group.title),
                        col_width as usize,
                        Style::default().fg(Color::Gray),
                    );
                    y_cur += 1;
                }

                // Print stacking nodes
                for &node_id in &group.node_ids {
                    if y_cur >= area.bottom() {
                        break;
                    }

                    if let Some(node) = self.graph.nodes.iter().find(|n| n.id == node_id) {
                        // Record position for edge rendering
                        node_positions.insert(node.id, (x_base, y_cur));

                        let selected = self.selected_node_id == Some(node.id);
                        let is_stacked =
                            col.key == FlowColumnKind::Tests || col.key == FlowColumnKind::Security;
                        let prefix = if selected {
                            ">>"
                        } else if is_stacked {
                            "│ "
                        } else {
                            "  "
                        };

                        let color = match node.status.as_str() {
                            "success" => Color::Green,
                            "running" => Color::Blue,
                            "failed" => Color::Red,
                            "pending" | "created" | "waiting_for_resource" | "preparing" => {
                                Color::Yellow
                            }
                            "canceled" => Color::DarkGray,
                            _ => Color::Gray,
                        };

                        let icon = match node.status.as_str() {
                            "success" => "✓",
                            "running" => "●",
                            "failed" => "✗",
                            "pending" | "created" | "waiting_for_resource" | "preparing" => "○",
                            "canceled" => "⊘",
                            _ => "◇",
                        };

                        // Build badge string from VTI / cache / flake / critical
                        let badges = build_badges(node);

                        // Truncate label to fit column
                        let badge_len = badges.len();
                        let max_label = (col_width as usize)
                            .saturating_sub(5) // prefix + icon + space
                            .saturating_sub(badge_len + 1);
                        let short_label = if node.label.len() > max_label && max_label > 3 {
                            format!("{}…", &node.label[..max_label.saturating_sub(1)])
                        } else {
                            node.label.clone()
                        };

                        let label = format!("{} {} {}", prefix, icon, short_label);
                        let mut style = Style::default().fg(color);
                        if node.is_critical_path && !selected {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if selected {
                            style = style.add_modifier(Modifier::REVERSED);
                        }

                        buf.set_stringn(x_base, y_cur, &label, col_width as usize, style);

                        // Render badges after the label
                        if !badges.is_empty() {
                            let badge_x = x_base + label.len().min(col_width as usize) as u16;
                            let remaining = area.right().saturating_sub(badge_x) as usize;
                            if remaining > 0 {
                                let badge_style = badge_style_for(node);
                                buf.set_stringn(
                                    badge_x,
                                    y_cur,
                                    format!(" {}", badges),
                                    remaining,
                                    badge_style,
                                );
                            }
                        }

                        y_cur += 1;
                    }
                }
            }
            if y_cur > max_y_drawn {
                max_y_drawn = y_cur;
            }
        }

        // Draw edges between columns
        draw_edges(buf, &self.graph.edges, &node_positions, area, col_width);
    }
}

/// Build the badge string for a node's VTI/cache/flake/agent status.
fn build_badges(node: &super::model::FlowNode) -> String {
    let mut parts = Vec::new();

    // Critical path badge
    if node.is_critical_path {
        parts.push("[CRIT]");
    }

    // VTI badges
    if let Some(ref vti) = node.vti_status {
        match vti {
            VtiStatus::Accelerated {
                time_saved_secs, ..
            } => {
                parts.push("[🔥 VTI]");
                if *time_saved_secs > 0 {
                    // We can't push a formatted string into &str vec, so just the badge
                }
            }
            VtiStatus::Skipped { .. } => parts.push("[SKIP]"),
            VtiStatus::Selected { .. } => parts.push("[SEL]"),
            VtiStatus::FullSuite => {}
        }
    }

    // Cache badges
    if let Some(ref cache) = node.cache_verdict {
        match cache {
            CacheVerdict::Hit { .. } => parts.push("[HIT]"),
            CacheVerdict::Tainted { .. } => parts.push("[TAINT]"),
            CacheVerdict::Denied { .. } => parts.push("[DENY]"),
            CacheVerdict::Miss => {}
        }
    }

    // Flake badge
    if let Some(flake) = node.flake_probability
        && flake > 0.15
    {
        parts.push("[FLK?]");
    }

    // Capsule badge
    if node.capsule_id.is_some() {
        parts.push("[CAP]");
    }

    // Agent badge
    if node.agent_id.is_some() {
        parts.push("[AGT]");
    }

    parts.join(" ")
}

/// Determine the badge color based on VTI/cache/critical status.
fn badge_style_for(node: &super::model::FlowNode) -> Style {
    if let Some(ref vti) = node.vti_status {
        match vti {
            VtiStatus::Accelerated { .. } => {
                return Style::default()
                    .fg(Color::Rgb(255, 165, 0)) // fire orange
                    .add_modifier(Modifier::BOLD);
            }
            VtiStatus::Skipped { .. } => {
                return Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD);
            }
            _ => {}
        }
    }
    if let Some(ref cache) = node.cache_verdict
        && matches!(cache, CacheVerdict::Tainted { .. })
    {
        return Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD);
    }
    if node.is_critical_path {
        return Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
    }
    Style::default().fg(Color::DarkGray)
}

/// Draw edges between connected nodes using edge-kind-specific glyphs.
fn draw_edges(
    buf: &mut Buffer,
    edges: &[super::model::FlowEdge],
    positions: &std::collections::HashMap<i64, (u16, u16)>,
    area: Rect,
    col_width: u16,
) {
    // For each edge, draw a connector between columns
    for edge in edges {
        let Some(&(from_x, from_y)) = positions.get(&edge.from) else {
            continue;
        };
        let Some(&(to_x, to_y)) = positions.get(&edge.to) else {
            continue;
        };

        // Only draw horizontal connections (between adjacent columns)
        if from_x >= to_x || from_y >= area.bottom() || to_y >= area.bottom() {
            continue;
        }

        let (connector, color) = edge_visual(edge.kind);

        // Draw the connector at the gap between columns
        let gap_x = from_x + col_width;
        if gap_x < to_x && gap_x < area.right() {
            // We have space in the gap — draw a short connector
            let draw_y = from_y.min(to_y);
            if draw_y < area.bottom() {
                let style = Style::default().fg(color);
                // Draw minimal connector character
                let avail = (to_x.saturating_sub(gap_x)) as usize;
                if avail > 0 {
                    buf.set_stringn(gap_x, draw_y, connector, avail, style);
                }
            }
        }
    }
}

/// Map edge kind to visual connector and color.
fn edge_visual(kind: EdgeKind) -> (&'static str, Color) {
    match kind {
        EdgeKind::GitlabNeeds => ("──▶", Color::White),
        EdgeKind::ArtifactDep => ("══▶", Color::Cyan),
        EdgeKind::StageOrder => ("- ▶", Color::DarkGray),
        EdgeKind::VtiSkipped => ("··▶", Color::DarkGray),
        EdgeKind::Blocked => ("✗─▶", Color::Red),
        EdgeKind::ChildPipeline => ("──▷", Color::Blue),
    }
}
