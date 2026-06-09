//! Agents lens view.
//!
//! Invariants: pure draw. Reads [`AgentsLensInput`]; no backend I/O. Renders
//! the agent fleet, live agent-run driver state, failed-CI repair workcells,
//! and a footer carrying the cursor plus a freeze/block alert.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use jeryu_readmodel::{AgentRunStatus, AgentStatus, WorkcellState};

use super::data::{AgentRow, AgentRunRow, AgentsLensInput, RepairCellRow};
use super::terminal::AgentTerminalSession;
use crate::widgets::terminal_pane;

pub fn draw(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    draw_with_terminal(f, input, None, area);
}

/// Draw the Agents lens, swapping the lifecycle table for the live terminal
/// pane when an attached session is present. With no session (or a detached
/// one) this is byte-identical to [`draw`].
pub fn draw_with_terminal(
    f: &mut Frame,
    input: &AgentsLensInput,
    terminal: Option<&AgentTerminalSession>,
    area: Rect,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / fleet summary
            Constraint::Length(8), // live runs + repair workcells
            Constraint::Min(0),    // session table / terminal pane
            Constraint::Length(3), // footer / cursor + alert
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_live_runs(f, input, chunks[1]);
    match terminal {
        Some(session) if session.is_attached() => terminal_pane::render(f, session, chunks[2]),
        _ => draw_body(f, input, chunks[2]),
    }
    draw_footer(f, input, chunks[3]);
}

fn draw_header(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let text = format!(
        "Agents — {} active · {} blocked · {} grants · {} running runs · {} live tty",
        input.active_agents,
        input.blocked_agents,
        input.active_grants,
        input.running_runs,
        input.live_tty_runs,
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Agents — Fleet "),
        ),
        area,
    );
}

fn status_style(status: AgentStatus) -> Style {
    match status {
        AgentStatus::Active => Style::default().fg(Color::Green),
        AgentStatus::Blocked => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        AgentStatus::Idle => Style::default().fg(Color::Gray),
        AgentStatus::Done => Style::default().fg(Color::DarkGray),
    }
}

fn run_status_style(status: AgentRunStatus) -> Style {
    match status {
        AgentRunStatus::Running => Style::default().fg(Color::Green),
        AgentRunStatus::Queued => Style::default().fg(Color::Yellow),
        AgentRunStatus::Finished => Style::default().fg(Color::DarkGray),
        AgentRunStatus::Failed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        AgentRunStatus::Cancelled => Style::default().fg(Color::Gray),
    }
}

fn workcell_style(state: WorkcellState) -> Style {
    match state {
        WorkcellState::Held | WorkcellState::Repairing => Style::default().fg(Color::Yellow),
        WorkcellState::Blocked => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        _ => Style::default(),
    }
}

fn draw_live_runs(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    draw_run_table(f, input, panes[0]);
    draw_repair_cells(f, input, panes[1]);
}

fn draw_run_table(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    if input.run_rows.is_empty() {
        f.render_widget(
            Paragraph::new("No active agent runs.")
                .block(Block::default().borders(Borders::ALL).title(" Agent Runs ")),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("RUN"),
        Cell::from("MODE"),
        Cell::from("TTY"),
        Cell::from("BUDGET"),
        Cell::from("CONTROL"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows: Vec<Row> = input.run_rows.iter().map(run_row).collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(15),
            Constraint::Min(16),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Agent Runs "));
    f.render_widget(table, area);
}

fn run_row(r: &AgentRunRow) -> Row<'_> {
    let tty = r.tty_status.clone().unwrap_or_else(|| "not tty".into());
    Row::new(vec![
        Cell::from(r.run_id.clone()),
        Cell::from(format!("{} {}", r.io_mode.label(), r.status.label()))
            .style(run_status_style(r.status)),
        Cell::from(tty),
        Cell::from(r.output_budget()),
        Cell::from(r.controls_label()),
    ])
}

fn draw_repair_cells(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    if input.repair_cells.is_empty() {
        f.render_widget(
            Paragraph::new("No held failed-CI workcells.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Repair Cells "),
            ),
            area,
        );
        return;
    }

    let mut lines = Vec::new();
    for cell in &input.repair_cells {
        lines.push(repair_line(cell));
        lines.push(Line::from(format!(
            "export={} receipt={}",
            cell.export_state.as_deref().unwrap_or("—"),
            cell.failed_receipt_id.as_deref().unwrap_or("—"),
        )));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Failed-CI Repair "),
        ),
        area,
    );
}

fn repair_line(r: &RepairCellRow) -> Line<'_> {
    Line::from(vec![
        Span::raw(format!("{} ", r.cell_id)),
        Span::styled(r.state.label().to_string(), workcell_style(r.state)),
        Span::raw(format!(
            " failed={} repair={}",
            r.failed_run_id.as_deref().unwrap_or("—"),
            r.repair_state.as_deref().unwrap_or("—"),
        )),
    ])
}

fn draw_body(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    if input.rows.is_empty() {
        let (can_code_text, can_code_style) = if input.agents_can_code {
            ("yes", Style::default().fg(Color::Green))
        } else {
            (
                "NO",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        };
        let lines = vec![
            Line::from("No live agent sessions."),
            Line::from(vec![
                Span::raw("Can code: "),
                Span::styled(can_code_text, can_code_style),
            ]),
            Line::from(format!("Active grants: {}", input.active_grants)),
        ];
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Lifecycle — {} ", input.fleet_status())),
            ),
            area,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from("SESSION"),
        Cell::from("STATUS"),
        Cell::from("TASK"),
        Cell::from("BRANCH"),
        Cell::from("GRANTS"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input.rows.iter().map(session_row).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(9),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Lifecycle — {} ", input.fleet_status())),
    );

    f.render_widget(table, area);
}

fn session_row(r: &AgentRow) -> Row<'_> {
    Row::new(vec![
        Cell::from(r.label.clone()),
        Cell::from(Span::styled(
            r.status.label().to_string(),
            status_style(r.status),
        )),
        Cell::from(r.current_task.clone().unwrap_or_else(|| "—".into())),
        Cell::from(r.branch.clone().unwrap_or_else(|| "—".into())),
        Cell::from(r.grants.to_string()),
    ])
}

fn draw_footer(f: &mut Frame, input: &AgentsLensInput, area: Rect) {
    let mut spans = vec![Span::raw(format!("cursor={}", input.event_cursor))];
    if !input.agents_can_code {
        spans.push(Span::styled(
            " · ❄ code frozen",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    } else if input.has_blocked() {
        spans.push(Span::styled(
            format!(" · ⚠ {} blocked", input.blocked_agents),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw(" · Keys: a actions · x explain · ? help"));
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{TuiReadModel, sample_read_model};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(w: u16, h: u16, input: &AgentsLensInput) -> String {
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
        let input = AgentsLensInput::from_read_model(&TuiReadModel::default());
        let out = ink(80, 24, &input);
        assert!(out.contains("Agents"));
        assert!(out.contains("Lifecycle"));
        assert!(out.contains("No live agent sessions."));
        assert!(out.contains("cursor=0"));
    }

    #[test]
    fn renders_sessions_at_120x36() {
        let input = AgentsLensInput::from_read_model(&sample_read_model());
        let out = ink(120, 36, &input);
        assert!(out.contains("agent-wrath-17"));
        assert!(out.contains("agent-storm-04"));
        assert!(out.contains("run-pty-18"));
        assert!(out.contains("live tty"));
        assert!(out.contains("send_input"));
        assert!(out.contains("wc-18"));
        assert!(out.contains("held_failed_ci"));
        assert!(out.contains("export_ready"));
        assert!(out.contains("blocked"));
        assert!(out.contains("1 blocked"));
        assert!(out.contains("feat/approvals"));
    }

    #[test]
    fn renders_frozen_fleet() {
        let mut model = TuiReadModel::default();
        model.mission.active_agents = 3;
        model.mission.agents_can_code = false;
        let input = AgentsLensInput::from_read_model(&model);
        let out = ink(120, 36, &input);
        assert!(out.contains("FROZEN"));
        assert!(out.contains("frozen"));
        assert!(out.contains("NO"));
    }

    #[test]
    fn renders_at_220x60_without_panic() {
        let input = AgentsLensInput::from_read_model(&sample_read_model());
        let _ = ink(220, 60, &input);
    }
}
