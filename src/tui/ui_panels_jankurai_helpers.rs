use super::*;

pub(super) fn visible_entry_window(
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

pub(super) fn scan_text(
    scan: Option<&crate::tui::jankurai::JankuraiScan>,
    value: impl FnOnce(&crate::tui::jankurai::JankuraiScan) -> String,
    absent: &'static str,
) -> String {
    match scan {
        Some(scan) => value(scan),
        None => absent.into(),
    }
}

pub(super) fn chart_labels(
    history: &[crate::tui::jankurai::JankuraiHistoryPoint],
) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
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

pub(super) fn y_axis_labels(lo: f64, hi: f64) -> Vec<Span<'static>> {
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

pub(super) fn format_timestamp(value: &chrono::DateTime<chrono::Utc>) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}
