//! Owner: Interactive TUI subsystem - Secrets lens view
//! Proof: `cargo test -p jeryu-tui --lib tui::lenses::secrets::view`
//! Invariants: Pure draw. Reads `SecretsLensInput`; never touches the DB,
//!             Vault, filesystem, or network during render. Preserves the
//!             legacy secrets-audit two-pane: a colored audit table on the
//!             left and a per-event detail / vault-status block on the right.
//!             SECURITY: renders ONLY audit metadata (time / action / status /
//!             repo). It never renders a secret value, rotation target, or any
//!             vaulted material — the lens input structurally cannot carry it.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use super::data::{SecretAuditRow, SecretsLensInput};

/// Vault-status copy shown in the detail pane when nothing is selected. Ported
/// verbatim from the legacy `draw_secrets_tab` so the posture message survives.
const VAULT_STATUS_TEXT: &str = "\n  Vault integration active.\n\n  Events appear here as secrets\n  are rotated, fetched, or revoked.\n\n  [RISK] = Security event requiring review.";

pub fn draw(f: &mut Frame, input: &SecretsLensInput, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / posture
            Constraint::Min(0),    // two-pane body (list + detail)
            Constraint::Length(3), // footer / keys
        ])
        .split(area);

    draw_header(f, input, chunks[0]);
    draw_body(f, input, chunks[1]);
    draw_footer(f, chunks[2]);
}

/// Color a status word the same way the legacy panel did: green for success,
/// red for failure, yellow for anything in-between (pending / unknown).
fn status_style(status: &str) -> Style {
    match status {
        "ok" | "success" => Style::default().fg(Color::Green),
        "error" | "failed" => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::Yellow),
    }
}

/// First 16 chars of the timestamp (`YYYY-MM-DDThh:mm`), matching the legacy ts.
fn short_ts(created_at: &str) -> &str {
    created_at.get(..16).unwrap_or(created_at)
}

fn draw_header(f: &mut Frame, input: &SecretsLensInput, area: Rect) {
    let n = input.events.len();
    let text = format!("Secrets — {n} audit events");
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Secrets — Vault Audit "),
        ),
        area,
    );
}

fn draw_body(f: &mut Frame, input: &SecretsLensInput, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_event_table(f, input, cols[0]);
    draw_detail(f, input, cols[1]);
}

fn draw_event_table(f: &mut Frame, input: &SecretsLensInput, area: Rect) {
    if input.events.is_empty() {
        f.render_widget(
            Paragraph::new("No secret audit events.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Secret Audit Events (0) "),
            ),
            area,
        );
        return;
    }

    let selected = input.clamped_selection();

    let header = Row::new(vec![
        Cell::from("TIME"),
        Cell::from("ACTION"),
        Cell::from("STATUS"),
        Cell::from("REPO"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = input
        .events
        .iter()
        .enumerate()
        .map(|(i, ev): (usize, &SecretAuditRow)| {
            let row = Row::new(vec![
                Cell::from(short_ts(&ev.created_at).to_string()),
                Cell::from(ev.action.clone()),
                Cell::from(Span::styled(ev.status.clone(), status_style(&ev.status))),
                Cell::from(ev.repo_name.clone()),
            ]);
            if Some(i) == selected {
                row.style(
                    Style::default()
                        .add_modifier(Modifier::REVERSED)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                row
            }
        })
        .collect();

    let title = format!(" Secret Audit Events ({}) ", input.events.len());
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(title));

    f.render_widget(table, area);
}

fn draw_detail(f: &mut Frame, input: &SecretsLensInput, area: Rect) {
    // SECURITY: the detail block surfaces only the four audit-metadata fields.
    // A secret value, the rotation target, and the version are intentionally
    // never shown (and are not even carried by `SecretAuditRow`).
    let detail_body = match input.selected_row() {
        Some(ev) => format!(
            "\n  Repo:     {}\n  Action:   {}\n  Status:   {}\n  Created:  {}\n",
            ev.repo_name, ev.action, ev.status, ev.created_at,
        ),
        None => VAULT_STATUS_TEXT.to_string(),
    };

    f.render_widget(
        Paragraph::new(detail_body)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Vault Status "),
            )
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let text = " · Keys: ↑/↓ select · enter detail · e evidence";
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Help ")),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::super::data::SecretAuditEvent;
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ev(action: &str, status: &str, repo: &str, created: &str) -> SecretAuditEvent {
        SecretAuditEvent {
            id: Some(1),
            repo_name: repo.into(),
            version: "v3.0.1".into(),
            target: "GITLAB_TOKEN".into(),
            action: action.into(),
            status: status.into(),
            detail: "TOP-SECRET-VALUE-MUST-NOT-RENDER".into(),
            created_at: created.into(),
        }
    }

    fn ink(width: u16, height: u16, input: &SecretsLensInput) -> String {
        let backend = TestBackend::new(width, height);
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

    fn populated() -> SecretsLensInput {
        let events = vec![
            ev("rotate", "ok", "jeryu", "2026-05-29T12:00:00Z"),
            ev("fetch", "error", "jankurai", "2026-05-29T12:05:00Z"),
            ev("revoke", "ok", "jeryu", "2026-05-29T12:10:00Z"),
        ];
        SecretsLensInput::from_state(&events, 1)
    }

    #[test]
    fn empty_renders_at_80x24_no_panic() {
        let input = SecretsLensInput::from_state(&[], 0);
        let s = ink(80, 24, &input);
        assert!(s.contains("Secrets"));
        assert!(s.contains("No secret audit events."));
    }

    #[test]
    fn populated_renders_at_80x24_with_columns() {
        let input = populated();
        let s = ink(80, 24, &input);
        assert!(s.contains("Secrets"));
        assert!(s.contains("TIME"));
        assert!(s.contains("ACTION"));
        assert!(s.contains("STATUS"));
        assert!(s.contains("REPO"));
        assert!(s.contains("audit events"));
    }

    #[test]
    fn populated_renders_at_120x36_with_detail() {
        let input = populated();
        let s = ink(120, 36, &input);
        assert!(s.contains("Secrets"));
        assert!(s.contains("ACTION"));
        // Selected row (index 1) detail surfaces its metadata.
        assert!(s.contains("jankurai"));
        assert!(s.contains("Action:"));
        assert!(s.contains("Status:"));
    }

    #[test]
    fn never_renders_secret_material() {
        let input = populated();
        for (w, h) in [(80u16, 24u16), (120, 36)] {
            let s = ink(w, h, &input);
            assert!(
                !s.contains("TOP-SECRET-VALUE-MUST-NOT-RENDER"),
                "secret detail value must never reach the screen"
            );
            assert!(
                !s.contains("GITLAB_TOKEN"),
                "rotation target must never reach the screen"
            );
        }
    }

    #[test]
    fn empty_detail_shows_vault_status() {
        let input = SecretsLensInput::from_state(&[], 0);
        let s = ink(120, 36, &input);
        assert!(s.contains("Vault integration active."));
    }
}
