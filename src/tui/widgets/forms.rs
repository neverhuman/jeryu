//! Owner: Interactive TUI subsystem — form primitives (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::forms`.
//! Invariants: Pure rendering. Three primitives — text input, checkbox,
//! select dropdown — each with focus-aware styling. Form state is owned
//! by the caller; widgets only paint pixels.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme::palette::Palette;

// ── Text input ──────────────────────────────────────────────────────────

pub struct TextInputProps<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub cursor: usize,
    pub focused: bool,
    pub placeholder: &'a str,
}

pub fn render_text_input(f: &mut Frame, props: &TextInputProps, area: Rect, palette: &Palette) {
    let border = if props.focused {
        palette.running
    } else {
        palette.stale
    };
    let block = Block::default()
        .title(format!(" {} ", props.label))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 2 || inner.height < 1 {
        return;
    }
    let visible: String = if props.value.is_empty() {
        props.placeholder.to_string()
    } else {
        props.value.to_string()
    };
    let style = if props.value.is_empty() {
        Style::default()
            .fg(palette.stale)
            .add_modifier(Modifier::ITALIC)
    } else {
        Style::default().fg(palette.running)
    };
    let mut line = vec![Span::styled(visible, style)];
    if props.focused && !props.value.is_empty() && props.cursor <= props.value.chars().count() {
        line.push(Span::styled(
            "▏",
            Style::default()
                .fg(palette.warn)
                .add_modifier(Modifier::BOLD | Modifier::SLOW_BLINK),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(line)), inner);
}

// ── Checkbox ────────────────────────────────────────────────────────────

pub struct CheckboxProps<'a> {
    pub label: &'a str,
    pub checked: bool,
    pub focused: bool,
}

pub fn render_checkbox(f: &mut Frame, props: &CheckboxProps, area: Rect, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let box_glyph = if props.checked { "[x]" } else { "[ ]" };
    let style = if props.focused {
        Style::default()
            .fg(palette.running)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(palette.stale)
    };
    let line = Line::from(vec![
        Span::styled(box_glyph, Style::default().fg(palette.warn)),
        Span::raw(" "),
        Span::styled(props.label.to_string(), style),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

// ── Select dropdown ─────────────────────────────────────────────────────

pub struct SelectProps<'a> {
    pub label: &'a str,
    pub options: &'a [&'a str],
    pub selected: usize,
    pub open: bool,
    pub focused: bool,
}

pub fn render_select(f: &mut Frame, props: &SelectProps, area: Rect, palette: &Palette) {
    let border = if props.focused {
        palette.running
    } else {
        palette.stale
    };
    let block = Block::default()
        .title(format!(" {} ", props.label))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width < 4 || inner.height < 1 {
        return;
    }

    let current = props.options.get(props.selected).copied().unwrap_or("—");
    let glyph = if props.open { "▾" } else { "▸" };
    let header = Line::from(vec![
        Span::styled(format!("{glyph} "), Style::default().fg(palette.warn)),
        Span::styled(current.to_string(), Style::default().fg(palette.running)),
    ]);
    let mut lines = vec![header];
    if props.open {
        for (i, opt) in props.options.iter().enumerate() {
            let active = i == props.selected;
            let style = if active {
                Style::default()
                    .fg(palette.ok)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(palette.stale)
            };
            lines.push(Line::from(Span::styled(format!("  {opt}"), style)));
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        term.draw(|f| {
            render_text_input(
                f,
                &TextInputProps {
                    label: "name",
                    value: "build-main",
                    cursor: 10,
                    focused: true,
                    placeholder: "type a name",
                },
                Rect::new(0, 0, 40, 3),
                &p,
            );
            render_checkbox(
                f,
                &CheckboxProps {
                    label: "redact",
                    checked: true,
                    focused: false,
                },
                Rect::new(0, 4, 40, 1),
                &p,
            );
            render_select(
                f,
                &SelectProps {
                    label: "transport",
                    options: &["http", "mcp", "local"],
                    selected: 1,
                    open: true,
                    focused: true,
                },
                Rect::new(0, 6, 40, 8),
                &p,
            );
        })
        .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        term.draw(|f| {
            render_text_input(
                f,
                &TextInputProps {
                    label: "name",
                    value: "",
                    cursor: 0,
                    focused: false,
                    placeholder: "type a name",
                },
                Rect::new(0, 0, 60, 3),
                &p,
            );
            render_select(
                f,
                &SelectProps {
                    label: "transport",
                    options: &["http", "mcp", "local"],
                    selected: 0,
                    open: false,
                    focused: false,
                },
                Rect::new(0, 4, 60, 3),
                &p,
            );
        })
        .unwrap();
    }
}
