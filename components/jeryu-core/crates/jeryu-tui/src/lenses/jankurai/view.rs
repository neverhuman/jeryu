//! Owner: Interactive TUI subsystem - Jankurai lens view
//! Proof: `cargo test -p jeryu-tui --lib lenses::jankurai::view`
//! Invariants: Pure draw. Reads `JankuraiLensInput` (an owned projection of
//!             `JankuraiSnapshot` + selected index); no I/O. Self-contained
//!             audit panel — every helper it needs (`scan_text`,
//!             `format_timestamp`, `chart_labels`, `y_axis_labels`,
//!             `visible_entry_window`, `short_text`, and the sparkline) is ported
//!             here as a private fn, so the lens depends on nothing in
//!             `ui_panels_*` or `ui_chrome`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph, Wrap,
};

use super::data::{
    JankuraiDimension, JankuraiEntry, JankuraiEntryKind, JankuraiHistoryPoint, JankuraiLensInput,
    JankuraiScan,
};

pub fn draw(f: &mut Frame, input: &JankuraiLensInput, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // summary + status
            Constraint::Length(10), // score chart + dimension breakdown
            Constraint::Min(8),     // caps/findings list + entry detail
            Constraint::Length(1),  // key hints footer
        ])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(rows[0]);
    let middle_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(rows[1]);
    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(rows[2]);

    render_summary_block(f, top_cols[0], input);
    render_status_block(f, top_cols[1], input);
    render_score_chart(f, middle_cols[0], &input.history);
    render_breakdown_block(f, middle_cols[1], &input.dimensions);
    render_issues_block(f, bottom_cols[0], &input.entries, input.selected_index);
    render_detail_block(f, bottom_cols[1], input.selected_entry());
    render_footer(f, rows[3]);
}

// ── Summary (top-left): score / raw / min / decision / status / findings / caps ─

fn render_summary_block(f: &mut Frame, area: Rect, input: &JankuraiLensInput) {
    let scan = input.last_scan.as_ref();

    let score_text = scan_text(scan, |s| s.score.to_string(), "n/a");
    let raw_score_text = scan_text(scan, |s| s.raw_score.to_string(), "n/a");
    let minimum_score_text = scan_text(scan, |s| s.minimum_score.to_string(), "n/a");
    let decision_text = scan_text(scan, |s| s.decision.clone(), "n/a");
    let score_status_text = scan_text(scan, |s| s.score_status.clone(), "n/a");
    let generated_at_text = match scan {
        Some(s) => match &s.generated_at {
            Some(ts) => format_timestamp(ts),
            None => "n/a".into(),
        },
        None => "n/a".into(),
    };
    let finding_count_text = scan_text(scan, |s| s.finding_count.to_string(), "0");
    let hard_findings_text = scan_text(scan, |s| s.hard_findings.to_string(), "0");
    let soft_findings_text = scan_text(scan, |s| s.soft_findings.to_string(), "0");
    let cap_count_text = scan_text(scan, |s| s.caps_applied.len().to_string(), "0");

    // Posture coloring: green when the score clears the minimum, red below.
    let score_color = match scan {
        Some(s) if s.score >= s.minimum_score => Color::Green,
        Some(_) => Color::Red,
        None => Color::DarkGray,
    };

    let summary_lines = vec![
        Line::from(vec![
            Span::styled("score:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                score_text,
                Style::default()
                    .fg(score_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   raw: ", Style::default().fg(Color::DarkGray)),
            Span::styled(raw_score_text, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("min:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(minimum_score_text, Style::default().fg(Color::Yellow)),
            Span::styled("   decision: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                decision_text,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("status:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(score_status_text, Style::default().fg(Color::Cyan)),
            Span::styled("   at: ", Style::default().fg(Color::DarkGray)),
            Span::styled(generated_at_text, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("findings:", Style::default().fg(Color::DarkGray)),
            Span::styled(finding_count_text, Style::default().fg(Color::Red)),
            Span::styled(" hard:", Style::default().fg(Color::DarkGray)),
            Span::styled(hard_findings_text, Style::default().fg(Color::Red)),
            Span::styled(" soft:", Style::default().fg(Color::DarkGray)),
            Span::styled(soft_findings_text, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("caps:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(cap_count_text, Style::default().fg(Color::Magenta)),
            Span::styled("   history points: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                input.history.len().to_string(),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let summary_block = Block::default()
        .title(" [ Jankurai Summary ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let summary_inner = summary_block.inner(area);
    f.render_widget(summary_block, area);

    if !input.has_scan() && input.error.is_none() {
        // Clean empty state mirroring the legacy available()/installed handling.
        let line = if input.installed {
            "  Jankurai installed — scan not run yet."
        } else {
            "  Jankurai not run yet / not installed."
        };
        f.render_widget(
            Paragraph::new(line).style(Style::default().fg(Color::DarkGray)),
            summary_inner,
        );
        return;
    }

    f.render_widget(Paragraph::new(summary_lines), summary_inner);
}

// ── Status (top-right): install / error posture ─────────────────────────────

fn render_status_block(f: &mut Frame, area: Rect, input: &JankuraiLensInput) {
    let status_block = Block::default()
        .title(" [ Jankurai Status ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if input.error.is_some() {
            Color::Red
        } else {
            Color::DarkGray
        }));
    let status_inner = status_block.inner(area);
    f.render_widget(status_block, area);

    if let Some(error) = &input.error {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Parse / load error",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    short_text(error, status_inner.width.saturating_sub(2) as usize),
                    Style::default().fg(Color::White),
                )),
            ])
            .wrap(Wrap { trim: false }),
            status_inner,
        );
    } else {
        let installed = if input.installed {
            "installed"
        } else {
            "not installed"
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Jankurai",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("PATH: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(installed, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("points: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        input.history.len().to_string(),
                        Style::default().fg(Color::Green),
                    ),
                ]),
            ])
            .wrap(Wrap { trim: false }),
            status_inner,
        );
    }
}

// ── Score history (middle-left): Chart, with sparkline fallback when small ──

fn render_score_chart(f: &mut Frame, area: Rect, history: &[JankuraiHistoryPoint]) {
    let chart_block = Block::default()
        .title(" [ Score History ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let chart_inner = chart_block.inner(area);
    f.render_widget(chart_block, area);

    if history.is_empty() {
        f.render_widget(
            Paragraph::new("  No Jankurai history found")
                .style(Style::default().fg(Color::DarkGray)),
            chart_inner,
        );
        return;
    }

    // Small areas: a compact text sparkline + range, like the legacy panel.
    if chart_inner.width < 40 || chart_inner.height < 6 {
        let scores: Vec<i64> = history.iter().map(|p| p.score).collect();
        let spark = spark_i64(
            &scores,
            chart_inner.width.saturating_sub(4) as usize,
            Color::Cyan,
        );
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("score: ", Style::default().fg(Color::DarkGray)),
                    spark,
                ]),
                Line::from(vec![
                    Span::styled("range: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(
                            "{} -> {}",
                            scores.iter().min().copied().unwrap_or(0),
                            scores.iter().max().copied().unwrap_or(0)
                        ),
                        Style::default().fg(Color::White),
                    ),
                ]),
            ]),
            chart_inner,
        );
        return;
    }

    let data: Vec<(f64, f64)> = history
        .iter()
        .enumerate()
        .map(|(i, p)| (i as f64, p.score as f64))
        .collect();
    let labels = chart_labels(history);
    // Zoom the Y-axis to the actual score range so the trend line is clearly
    // visible instead of appearing flat at the top of a 0-100 scale.
    let y_min = data.iter().map(|(_, y)| *y).fold(f64::INFINITY, f64::min);
    let y_max = data
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::NEG_INFINITY, f64::max);
    let y_pad = ((y_max - y_min) * 0.3).max(5.0);
    let y_lo = (y_min - y_pad).max(0.0);
    let y_hi = (y_max + y_pad).min(100.0);
    let chart = Chart::new(vec![
        Dataset::default()
            .name("score")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&data),
    ])
    .block(Block::default())
    .x_axis(
        Axis::default()
            .title("time")
            .style(Style::default().fg(Color::DarkGray))
            .bounds([0.0, (data.len().saturating_sub(1)).max(1) as f64])
            .labels(labels.0),
    )
    .y_axis(
        Axis::default()
            .title("score")
            .style(Style::default().fg(Color::DarkGray))
            .bounds([y_lo, y_hi])
            .labels(y_axis_labels(y_lo, y_hi)),
    );
    f.render_widget(chart, chart_inner);
}

// ── Dimension breakdown (middle-right) ──────────────────────────────────────

fn render_breakdown_block(f: &mut Frame, area: Rect, dimensions: &[JankuraiDimension]) {
    let block = Block::default()
        .title(" [ Last Scan Dimensions ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if dimensions.is_empty() {
        f.render_widget(
            Paragraph::new("  No dimension breakdown available")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let lines = dimensions
        .iter()
        .map(|dimension| {
            let notes = if dimension.notes.is_empty() {
                String::new()
            } else {
                format!(" notes: {}", short_text(&dimension.notes.join("; "), 40))
            };
            Line::from(vec![
                Span::styled(
                    format!("{:>3} ", dimension.score),
                    Style::default().fg(if dimension.score >= 90 {
                        Color::Green
                    } else if dimension.score >= 75 {
                        Color::Yellow
                    } else {
                        Color::Red
                    }),
                ),
                Span::styled(
                    format!("w{:>2} ", dimension.weight),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    short_text(&dimension.name, inner.width.saturating_sub(16) as usize),
                    Style::default().fg(Color::White),
                ),
                Span::styled(notes, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

// ── Caps / Findings list (bottom-left) with selection highlight ─────────────

fn render_issues_block(
    f: &mut Frame,
    area: Rect,
    entries: &[JankuraiEntry],
    selected_index: usize,
) {
    let block = Block::default()
        .title(" [ Caps / Findings ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (visible_start, visible_end) =
        visible_entry_window(entries.len(), selected_index, inner.height as usize);
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .skip(visible_start)
        .take(visible_end.saturating_sub(visible_start))
        .map(|(index, entry)| {
            let selected = index == selected_index;
            let style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };
            let (badge, badge_color) = match entry.kind {
                JankuraiEntryKind::Cap => ("CAP", Color::Magenta),
                JankuraiEntryKind::Finding => match entry.severity.as_deref() {
                    Some("high") => ("HIGH", Color::Red),
                    Some("medium") => ("MED", Color::Yellow),
                    Some("low") => ("LOW", Color::Green),
                    _ => ("INFO", Color::Gray),
                },
            };
            let line = Line::from(vec![
                Span::styled(
                    format!(" {:<5} ", badge),
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "{:<18} ",
                        short_text(entry.path.as_deref().unwrap_or(""), 18)
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    short_text(
                        entry.problem.as_deref().unwrap_or(&entry.label),
                        inner.width.saturating_sub(32) as usize,
                    ),
                    Style::default().fg(Color::White),
                ),
            ]);
            ListItem::new(line).style(style)
        })
        .collect();

    if items.is_empty() {
        f.render_widget(
            Paragraph::new("  No caps or findings recorded.")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
    } else {
        f.render_widget(List::new(items), inner);
    }
}

// ── Entry detail (bottom-right) ─────────────────────────────────────────────

fn render_detail_block(f: &mut Frame, area: Rect, entry: Option<&JankuraiEntry>) {
    let block = Block::default()
        .title(" [ Entry Detail ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(entry) = entry else {
        f.render_widget(
            Paragraph::new("  No Jankurai entry selected.")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("kind:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                match entry.kind {
                    JankuraiEntryKind::Cap => "cap",
                    JankuraiEntryKind::Finding => "finding",
                },
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        detail_line("rule:    ", entry.rule.as_deref(), Color::White),
        detail_line("path:    ", entry.path.as_deref(), Color::White),
        detail_line("lane:    ", entry.lane.as_deref(), Color::White),
        detail_line("owner:   ", entry.owner.as_deref(), Color::White),
        detail_line("severity:", entry.severity.as_deref(), Color::White),
        detail_line("hardness:", entry.hardness.as_deref(), Color::White),
        Line::from(Span::styled(
            "────────────────────────",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(vec![
            Span::styled("problem: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                short_text(
                    entry.problem.as_deref().unwrap_or("n/a"),
                    inner.width.saturating_sub(11) as usize,
                ),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("fix:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                short_text(
                    entry.suggested_fix.as_deref().unwrap_or("n/a"),
                    inner.width.saturating_sub(11) as usize,
                ),
                Style::default().fg(Color::Yellow),
            ),
        ]),
    ];

    if !entry.evidence.is_empty() {
        lines.push(Line::from(Span::styled(
            "evidence:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for item in &entry.evidence {
            lines.push(Line::from(Span::styled(
                format!(
                    "  - {}",
                    short_text(item, inner.width.saturating_sub(6) as usize)
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn detail_line(label: &'static str, value: Option<&str>, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::DarkGray)),
        Span::styled(
            value.unwrap_or("n/a").to_string(),
            Style::default().fg(color),
        ),
    ])
}

// ── Footer: key hints ───────────────────────────────────────────────────────

fn render_footer(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " · Keys: ↑/↓ select · e evidence · ? help",
            Style::default().fg(Color::DarkGray),
        ))),
        area,
    );
}

// ── Ported helpers (self-contained; no ui_panels_*/ui_chrome dependency) ────

/// Resolve a scan field to text, or a fixed `absent` string when no scan.
/// Ported from `ui_panels_jankurai_helpers::scan_text`.
fn scan_text(
    scan: Option<&JankuraiScan>,
    value: impl FnOnce(&JankuraiScan) -> String,
    absent: &'static str,
) -> String {
    match scan {
        Some(scan) => value(scan),
        None => absent.into(),
    }
}

/// Ported from `ui_panels_jankurai_helpers::format_timestamp`.
fn format_timestamp(value: &chrono::DateTime<chrono::Utc>) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}

/// Ported from `ui_panels_jankurai_helpers::chart_labels`.
fn chart_labels(history: &[JankuraiHistoryPoint]) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let start = match history.first() {
        Some(point) => format_timestamp(&point.generated_at),
        None => "start".into(),
    };
    let end = match history.last() {
        Some(point) => format_timestamp(&point.generated_at),
        None => "end".into(),
    };
    (
        vec![
            Span::styled(start, Style::default().fg(Color::DarkGray)),
            Span::styled(end, Style::default().fg(Color::DarkGray)),
        ],
        vec![],
    )
}

/// Ported from `ui_panels_jankurai_helpers::y_axis_labels`.
fn y_axis_labels(lo: f64, hi: f64) -> Vec<Span<'static>> {
    let mid = ((lo + hi) / 2.0).round() as i64;
    vec![
        Span::styled(
            format!("{}", lo.round() as i64),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("{}", mid), Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", hi.round() as i64),
            Style::default().fg(Color::DarkGray),
        ),
    ]
}

/// Compute the visible entry window, keeping the selection centered.
/// Ported from `ui_panels_jankurai_helpers::visible_entry_window`.
fn visible_entry_window(
    entry_count: usize,
    selected_index: usize,
    row_count: usize,
) -> (usize, usize) {
    if entry_count == 0 || row_count == 0 {
        return (0, 0);
    }
    let visible_count = row_count.min(entry_count);
    let selected = selected_index.min(entry_count - 1);
    let mut start = selected.saturating_sub(visible_count / 2);
    if start + visible_count > entry_count {
        start = entry_count - visible_count;
    }
    (start, start + visible_count)
}

/// Truncate to `max_chars`, appending an ellipsis when clipped.
/// Ported from `ui_panels_body_logs::short_text`.
fn short_text(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let text = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", text)
    } else {
        text
    }
}

const SPARK_BLOCKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render a compact block sparkline from integer scores.
/// Ported from `tui::widgets::sparkline::{spark_i64, spark_str}` so the lens is
/// self-contained.
fn spark_i64(values: &[i64], width: usize, color: Color) -> Span<'static> {
    Span::styled(spark_str(values, width), Style::default().fg(color))
}

fn spark_str(values: &[i64], width: usize) -> String {
    if values.is_empty() || width == 0 {
        return "n/a".to_string();
    }
    let take = width.min(values.len());
    let slice = &values[values.len() - take..];
    let min = slice.iter().copied().min().unwrap_or(0);
    let max = slice.iter().copied().max().unwrap_or(0);
    if max == min {
        return SPARK_BLOCKS[3].to_string().repeat(take);
    }
    let span = (max - min) as f64;
    slice
        .iter()
        .map(|v| {
            let normalized = (((*v - min) as f64 / span) * 7.0).round() as usize;
            SPARK_BLOCKS[normalized.min(7)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::data::{
        JankuraiDimension, JankuraiEntry, JankuraiEntryKind, JankuraiHistoryPoint, JankuraiScan,
        JankuraiSnapshot,
    };
    use super::*;
    use chrono::TimeZone;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn ink(input: &JankuraiLensInput, w: u16, h: u16) -> String {
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

    fn populated_input() -> JankuraiLensInput {
        let ts = chrono::Utc.timestamp_opt(1_778_038_040, 0).unwrap();
        let snapshot = JankuraiSnapshot {
            installed: true,
            history: vec![
                JankuraiHistoryPoint {
                    generated_at: ts,
                    score: 80,
                    raw_score: Some(80),
                    decision: Some("advisory".into()),
                },
                JankuraiHistoryPoint {
                    generated_at: ts,
                    score: 92,
                    raw_score: Some(92),
                    decision: Some("advisory".into()),
                },
            ],
            dimensions: vec![JankuraiDimension {
                name: "tests".into(),
                weight: 30,
                score: 88,
                weighted_points: 26.4,
                evidence: vec!["coverage 88%".into()],
                notes: vec!["raise coverage".into()],
            }],
            entries: vec![
                JankuraiEntry {
                    kind: JankuraiEntryKind::Finding,
                    label: "missing test".into(),
                    severity: Some("high".into()),
                    hardness: Some("hard".into()),
                    path: Some("src/lib.rs".into()),
                    rule: Some("rule-a".into()),
                    lane: Some("fast".into()),
                    owner: Some("tools".into()),
                    problem: Some("no coverage on parser".into()),
                    evidence: vec!["line 42".into()],
                    suggested_fix: Some("add a unit test".into()),
                },
                JankuraiEntry {
                    kind: JankuraiEntryKind::Cap,
                    label: "cap-a".into(),
                    severity: None,
                    hardness: None,
                    path: None,
                    rule: None,
                    lane: None,
                    owner: None,
                    problem: None,
                    evidence: vec![],
                    suggested_fix: None,
                },
            ],
            last_scan: Some(JankuraiScan {
                generated_at: Some(ts),
                score: 92,
                raw_score: 92,
                minimum_score: 85,
                decision: "advisory".into(),
                score_status: "pass".into(),
                finding_count: 1,
                hard_findings: 1,
                soft_findings: 0,
                caps_applied: vec!["cap-a".into()],
            }),
            error: None,
        };
        JankuraiLensInput::from_state(&snapshot, 0)
    }

    #[test]
    fn renders_populated_at_80x24_without_panic() {
        let ink = ink(&populated_input(), 80, 24);
        assert!(ink.contains("Jankurai"), "header must name Jankurai");
        assert!(ink.contains("score"), "summary must mention score");
        assert!(ink.contains("decision"), "summary must mention decision");
    }

    #[test]
    fn renders_populated_at_120x40_without_panic() {
        let ink = ink(&populated_input(), 120, 40);
        assert!(ink.contains("Jankurai"));
        assert!(ink.contains("score"));
        assert!(ink.contains("decision"));
        assert!(ink.contains("Dimensions"), "dimension pane must render");
        assert!(ink.contains("Caps"), "findings pane must render");
    }

    #[test]
    fn empty_state_renders_clean_not_installed_line() {
        let input = JankuraiLensInput::from_state(&JankuraiSnapshot::default(), 0);
        let ink = ink(&input, 80, 24);
        assert!(ink.contains("Jankurai"));
        assert!(
            ink.contains("not installed"),
            "empty state must show a clean not-installed line"
        );
        // No apology / placeholder strings leak through.
        assert!(!ink.contains("TODO"));
        assert!(!ink.contains("placeholder"));
    }

    #[test]
    fn error_state_renders_parse_error() {
        let snapshot = JankuraiSnapshot {
            installed: true,
            error: Some("repo-score.json: invalid json".into()),
            ..Default::default()
        };
        let input = JankuraiLensInput::from_state(&snapshot, 0);
        let ink = ink(&input, 120, 40);
        assert!(ink.contains("error"), "error posture must be visible");
    }

    #[test]
    fn selected_index_out_of_range_does_not_panic() {
        let input = JankuraiLensInput::from_state(&JankuraiSnapshot::default(), 999);
        let _ = ink(&input, 100, 30);
    }
}
