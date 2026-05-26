//! Owner: Interactive TUI subsystem — `?` help overlay (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::help`.
//! Invariants: Pure rendering. Caller provides the keybinding table; widget
//! owns layout only. Groups are rendered as section headers + sorted entries.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::tui::theme::palette::Palette;

#[derive(Debug, Clone, Copy)]
pub struct KeyBinding<'a> {
    pub key: &'a str,
    pub desc: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct HelpGroup<'a> {
    pub title: &'a str,
    pub bindings: &'a [KeyBinding<'a>],
}

pub struct HelpProps<'a> {
    pub title: &'a str,
    pub groups: &'a [HelpGroup<'a>],
}

pub fn render(f: &mut Frame, props: &HelpProps, area: Rect, palette: &Palette) {
    let w = area.width.min(72);
    let h = area.height.min(28);
    if w < 16 || h < 4 {
        return;
    }
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let overlay = Rect::new(x, y, w, h);
    f.render_widget(Clear, overlay);

    let block = Block::default()
        .title(format!(" {} ", props.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(palette.cache));
    let inner = block.inner(overlay);
    f.render_widget(block, overlay);

    let mut lines: Vec<Line> = Vec::new();
    for grp in props.groups {
        lines.push(Line::from(Span::styled(
            format!(" {} ", grp.title),
            Style::default()
                .fg(palette.warn)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )));
        for b in grp.bindings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<10}", b.key),
                    Style::default()
                        .fg(palette.running)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(b.desc.to_string(), Style::default().fg(palette.stale)),
            ]));
        }
        lines.push(Line::from(""));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample() -> HelpProps<'static> {
        const NAV: &[KeyBinding<'static>] = &[
            KeyBinding {
                key: "Tab",
                desc: "next pane",
            },
            KeyBinding {
                key: "↑/↓",
                desc: "move",
            },
            KeyBinding {
                key: "Enter",
                desc: "drill in",
            },
            KeyBinding {
                key: "Esc",
                desc: "back",
            },
        ];
        const CMD: &[KeyBinding<'static>] = &[
            KeyBinding {
                key: ":",
                desc: "command palette",
            },
            KeyBinding {
                key: "?",
                desc: "this help",
            },
            KeyBinding {
                key: "q",
                desc: "quit",
            },
        ];
        const GROUPS: &[HelpGroup<'static>] = &[
            HelpGroup {
                title: "Navigation",
                bindings: NAV,
            },
            HelpGroup {
                title: "Global",
                bindings: CMD,
            },
        ];
        HelpProps {
            title: "Keybindings",
            groups: GROUPS,
        }
    }

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        term.draw(|f| render(f, &sample(), f.area(), &p)).unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        term.draw(|f| render(f, &sample(), f.area(), &p)).unwrap();
    }
}
