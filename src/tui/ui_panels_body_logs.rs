use super::*;

// ---------------------------------------------------------------------------
// Log text rendering helpers
// ---------------------------------------------------------------------------

pub(crate) fn redact_log_line(line: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let mut result = Cow::Borrowed(line);
    let patterns: &[(&str, &str)] = &[("glpat-", "glpat-[REDACTED]"), ("hvs.", "hvs.[REDACTED]")];
    for (prefix, replacement) in patterns {
        if let Some(start) = result.find(prefix) {
            let s = result.into_owned();
            let end = s[start + prefix.len()..]
                .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                .map(|i| start + prefix.len() + i)
                .unwrap_or(s.len());
            result = Cow::Owned(format!("{}{}{}", &s[..start], replacement, &s[end..]));
        }
    }
    // Redact URL credentials: ://user:token@
    if result.contains("://") && result.contains('@') {
        let s = result.into_owned();
        let redacted = regex_redact_url_creds(&s);
        result = Cow::Owned(redacted);
    }
    result
}

pub(crate) fn regex_redact_url_creds(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("://") {
        let after = &rest[pos + 3..];
        out.push_str(&rest[..pos + 3]);
        if let Some(at_pos) = after.find('@') {
            if let Some(colon_pos) = after[..at_pos].find(':') {
                out.push_str(&after[..colon_pos + 1]);
                out.push_str("[REDACTED]");
                rest = &after[at_pos..];
            } else {
                out.push_str(&after[..at_pos]);
                rest = &after[at_pos..];
            }
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn render_log_text(log: &str) -> Text<'static> {
    if log.contains('\x1b') {
        use ansi_to_tui::IntoText;
        if let Ok(text) = log.into_text() {
            let redacted_lines: Vec<Line<'static>> = text
                .lines
                .into_iter()
                .map(|line| {
                    let raw: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    let redacted = redact_log_line(&raw);
                    if redacted.as_ref() != raw.as_str() {
                        Line::from(Span::raw(redacted.into_owned()))
                    } else {
                        Line::from(
                            line.spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.into_owned(), s.style))
                                .collect::<Vec<_>>(),
                        )
                    }
                })
                .collect();
            return Text::from(redacted_lines);
        }
    }
    highlight_plain_log(log)
}

pub(crate) fn highlight_plain_log(log: &str) -> Text<'static> {
    let lines = log
        .lines()
        .map(|line| {
            let line = redact_log_line(line).into_owned();
            let line = line.as_str();
            let lower = line.to_lowercase();
            let style = if lower.contains("error")
                || lower.contains("failed")
                || lower.contains("panic")
                || lower.contains("fatal")
            {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else if lower.contains("warning") || lower.contains("warn") {
                Style::default().fg(Color::Yellow)
            } else if lower.contains("success")
                || lower.contains("passed")
                || lower.ends_with(" ok")
                || lower.contains(" finished ")
            {
                Style::default().fg(Color::Green)
            } else if lower.starts_with('$')
                || lower.starts_with('+')
                || lower.contains("cargo ")
                || lower.contains("docker ")
            {
                Style::default().fg(Color::Cyan)
            } else if lower.starts_with('[') || lower.contains("t00:") {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(line.to_string(), style))
        })
        .collect::<Vec<_>>();
    Text::from(lines)
}

// ---------------------------------------------------------------------------
// String utilities
// ---------------------------------------------------------------------------

pub(crate) fn short_text(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let text = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", text)
    } else {
        text
    }
}

#[allow(dead_code)]
pub(crate) fn format_duration(secs: i64) -> String {
    if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

// ---------------------------------------------------------------------------
// Command Palette overlay (Ctrl-K)
// ---------------------------------------------------------------------------

#[path = "ui_panels_body_palette.rs"]
mod ui_panels_body_palette;
pub(crate) use ui_panels_body_palette::*;
