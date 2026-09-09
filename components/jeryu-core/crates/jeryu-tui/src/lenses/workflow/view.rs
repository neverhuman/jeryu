//! Workflow lens view.
//!
//! Invariants: pure draw. Reads [`WorkflowLensInput`]; no backend I/O. Renders
//! the Workflow Atlas: a fleet header, a per-workflow delivery table (REPO/PR/
//! POSTURE/CRITICAL PATH, posture colored), and a keys footer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::DeliveryPosture;

use super::data::{WorkflowLensInput, WorkflowRow};

pub fn draw(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // fleet header
            Constraint::Min(0),    // delivery table
            Constraint::Length(3), // keys footer
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_atlas(f, input, chunks[1]);
    draw_footer(f, input, chunks[2]);
}

fn posture_style(posture: DeliveryPosture) -> Style {
    match posture {
        DeliveryPosture::Running => Style::default().fg(Color::Green),
        DeliveryPosture::Blocked => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        DeliveryPosture::Merging => Style::default().fg(Color::Cyan),
        DeliveryPosture::Delivered => Style::default().fg(Color::Blue),
        DeliveryPosture::Idle => Style::default().fg(Color::Gray),
    }
}

fn draw_header(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let text = format!(
        "Workflow Atlas — {} workflows · {} running · {} blocked · longest {}s",
        input.pipeline_count(),
        input.running_count,
        input.blocked_count,
        input.longest_running_seconds,
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Workflow Atlas — Delivery Posture "),
        ),
        area,
    );
}

fn draw_atlas(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    if input.rows.is_empty() {
        f.render_widget(
            Paragraph::new("No active delivery workflows.")
                .block(Block::default().borders(Borders::ALL).title(" Atlas ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("REPO"),
        Cell::from("PR"),
        Cell::from("POSTURE"),
        Cell::from("CRITICAL PATH"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input.rows.iter().map(pipeline_row).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(16),
            Constraint::Length(8),
            Constraint::Length(11),
            Constraint::Min(16),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Atlas "));

    f.render_widget(table, area);
}

fn pipeline_row(r: &WorkflowRow) -> Row<'_> {
    let pr = match r.pr_number {
        Some(n) => format!("#{n}"),
        None => "—".into(),
    };
    Row::new(vec![
        Cell::from(r.repo_slug.clone()),
        Cell::from(pr),
        Cell::from(Span::styled(
            r.posture.label().to_string(),
            posture_style(r.posture),
        )),
        Cell::from(r.critical_path_node.clone().unwrap_or_else(|| "—".into())),
    ])
}

fn draw_footer(f: &mut Frame, input: &WorkflowLensInput, area: Rect) {
    let text = format!(
        "cursor={} · Keys: enter drill · g graph · e evidence",
        input.event_cursor
    );
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Keys ")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &WorkflowLensInput) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, input, f.area())).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn renders_empty_at_80x24() {
        let input = WorkflowLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Workflow Atlas"));
        assert!(out.contains("No active delivery workflows."));
        assert!(out.contains("cursor=0"));
    }

    #[test]
    fn renders_workflows_at_120x36() {
        let input = WorkflowLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input);
        assert!(out.contains("core/web"));
        assert!(out.contains("core/api"));
        assert!(out.contains("#101"));
        assert!(out.contains("running"));
        assert!(out.contains("BLOCKED"));
        assert!(out.contains("1 running"));
        assert!(out.contains("ci:build-web"));
        // User-facing vocab: header counts "workflows", not "pipelines".
        assert!(out.contains("2 workflows"));
        assert!(!out.contains("pipelines"));
    }

    #[test]
    fn renders_no_placeholder_markers() {
        let input = WorkflowLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input).to_lowercase();
        assert!(!out.contains("placeholder"));
        assert!(!out.contains("not yet ported"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = WorkflowLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
