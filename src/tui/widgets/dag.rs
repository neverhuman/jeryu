//! Owner: Interactive TUI subsystem — generic DAG canvas widget (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::dag`.
//! Invariants: Pure rendering. Nodes are placed by caller-supplied (col,row)
//! coordinates; edges are drawn with explicit/inferred styling. This widget
//! owns layout primitives only — pipeline-specific DAGs (workflow lens)
//! compose it.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::palette::Palette;

/// A DAG node positioned in a logical grid. `col` is column (left→right),
/// `row` is row (top→bottom). The renderer maps grid coords to character
/// cells inside `area`.
#[derive(Debug, Clone)]
pub struct DagNode<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub col: u16,
    pub row: u16,
    pub state: NodeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    Ok,
    Running,
    Failed,
    Blocked,
    Stale,
}

impl NodeState {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Ok => "✓",
            Self::Running => "●",
            Self::Failed => "✗",
            Self::Blocked => "⊘",
            Self::Stale => "~",
        }
    }

    pub fn color(self, p: &Palette) -> ratatui::style::Color {
        match self {
            Self::Ok => p.ok,
            Self::Running => p.running,
            Self::Failed => p.crit,
            Self::Blocked => p.warn,
            Self::Stale => p.stale,
        }
    }
}

/// A directed edge between two node IDs. `inferred` toggles dashed styling.
#[derive(Debug, Clone, Copy)]
pub struct DagEdge<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub inferred: bool,
}

pub struct DagProps<'a> {
    pub title: &'a str,
    pub nodes: &'a [DagNode<'a>],
    pub edges: &'a [DagEdge<'a>],
    pub cell_w: u16,
    pub cell_h: u16,
}

pub fn render(f: &mut Frame, props: &DagProps, area: Rect, palette: &Palette) {
    let block = Block::default()
        .title(format!(" {} ", props.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.stale));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 2 || props.cell_w == 0 || props.cell_h == 0 {
        return;
    }

    let buf = f.buffer_mut();

    // Build a node-id → (x, y) index for edge routing.
    let mut positions: Vec<(&str, u16, u16)> = Vec::with_capacity(props.nodes.len());
    for n in props.nodes {
        let x = inner.x.saturating_add(n.col.saturating_mul(props.cell_w));
        let y = inner.y.saturating_add(n.row.saturating_mul(props.cell_h));
        if x < inner.right() && y < inner.bottom() {
            positions.push((n.id, x, y));
        }
    }

    // Draw edges first (under nodes).
    for e in props.edges {
        let from = positions.iter().find(|(id, _, _)| *id == e.from);
        let to = positions.iter().find(|(id, _, _)| *id == e.to);
        let (Some(&(_, fx, fy)), Some(&(_, tx, ty))) = (from, to) else {
            continue;
        };
        draw_edge(buf, fx, fy, tx, ty, e.inferred, palette, inner);
    }

    // Draw nodes.
    for n in props.nodes {
        let pos = positions.iter().find(|(id, _, _)| *id == n.id);
        let Some(&(_, x, y)) = pos else { continue };
        let style = Style::default()
            .fg(n.state.color(palette))
            .add_modifier(Modifier::BOLD);
        let label = format!("{} {}", n.state.glyph(), n.label);
        let line = Line::from(Span::styled(label, style));
        let max_w = inner.right().saturating_sub(x);
        let cell = Rect::new(x, y, max_w, 1);
        f.render_widget(Paragraph::new(line), cell);
    }
}

fn draw_edge(
    buf: &mut ratatui::buffer::Buffer,
    fx: u16,
    fy: u16,
    tx: u16,
    ty: u16,
    inferred: bool,
    palette: &Palette,
    bounds: Rect,
) {
    let ch = if inferred { '┄' } else { '─' };
    let style = Style::default().fg(if inferred { palette.stale } else { palette.running });
    // Horizontal segment.
    let (x0, x1) = if fx <= tx { (fx, tx) } else { (tx, fx) };
    for x in (x0 + 1)..x1 {
        if x >= bounds.right() || fy >= bounds.bottom() {
            break;
        }
        buf[(x, fy)].set_char(ch).set_style(style);
    }
    // Vertical segment.
    let (y0, y1) = if fy <= ty { (fy, ty) } else { (ty, fy) };
    let vch = if inferred { '┊' } else { '│' };
    for y in (y0 + 1)..=y1 {
        if tx >= bounds.right() || y >= bounds.bottom() {
            break;
        }
        buf[(tx, y)].set_char(vch).set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample() -> (Vec<DagNode<'static>>, Vec<DagEdge<'static>>) {
        let nodes = vec![
            DagNode {
                id: "a",
                label: "build",
                col: 0,
                row: 0,
                state: NodeState::Ok,
            },
            DagNode {
                id: "b",
                label: "test",
                col: 1,
                row: 0,
                state: NodeState::Running,
            },
            DagNode {
                id: "c",
                label: "deploy",
                col: 2,
                row: 1,
                state: NodeState::Blocked,
            },
        ];
        let edges = vec![
            DagEdge {
                from: "a",
                to: "b",
                inferred: false,
            },
            DagEdge {
                from: "b",
                to: "c",
                inferred: true,
            },
        ];
        (nodes, edges)
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let (nodes, edges) = sample();
        let props = DagProps {
            title: "Pipeline DAG",
            nodes: &nodes,
            edges: &edges,
            cell_w: 14,
            cell_h: 3,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 80, 18), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let (nodes, edges) = sample();
        let props = DagProps {
            title: "Pipeline DAG",
            nodes: &nodes,
            edges: &edges,
            cell_w: 20,
            cell_h: 4,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 120, 30), &p))
            .unwrap();
    }

    #[test]
    fn node_state_glyphs_are_total() {
        for s in [
            NodeState::Ok,
            NodeState::Running,
            NodeState::Failed,
            NodeState::Blocked,
            NodeState::Stale,
        ] {
            assert!(!s.glyph().is_empty());
        }
    }
}
