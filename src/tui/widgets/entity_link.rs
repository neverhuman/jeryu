//! Owner: Interactive TUI subsystem — entity link span (U13).
//! Proof: `cargo nextest run -p jeryu --lib tui::widgets::entity_link`.
//! Invariants: Pure rendering. Wraps `EntityRef::display()` as a styled
//! link-style span suitable for inline use in Lines.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::api::entity::EntityRef;
use crate::tui::theme::palette::Palette;

pub struct EntityLinkProps<'a> {
    pub entity: &'a EntityRef,
    pub focused: bool,
}

/// Build the styled span for an entity link. Caller composes it into Lines.
pub fn link_span(props: &EntityLinkProps, palette: &Palette) -> Span<'static> {
    let mut style = Style::default().fg(palette.cache);
    style = style.add_modifier(Modifier::UNDERLINED);
    if props.focused {
        style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
    }
    Span::styled(props.entity.display(), style)
}

/// Render a single entity link into `area` (occupies one cell row).
pub fn render(f: &mut Frame, props: &EntityLinkProps, area: Rect, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(Line::from(link_span(props, palette))),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::EntityKind;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_at_80x24() {
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let entity = EntityRef::new(EntityKind::Job, "14445");
        let props = EntityLinkProps {
            entity: &entity,
            focused: true,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 30, 1), &p))
            .unwrap();
    }

    #[test]
    fn renders_at_120x36() {
        let backend = TestBackend::new(120, 36);
        let mut term = Terminal::new(backend).unwrap();
        let p = Palette::dark();
        let entity = EntityRef::new(EntityKind::Agent, "wrath-17");
        let props = EntityLinkProps {
            entity: &entity,
            focused: false,
        };
        term.draw(|f| render(f, &props, Rect::new(0, 0, 30, 1), &p))
            .unwrap();
    }

    #[test]
    fn span_uses_entity_display_format() {
        let p = Palette::dark();
        let entity = EntityRef::new(EntityKind::Job, "14445");
        let props = EntityLinkProps {
            entity: &entity,
            focused: false,
        };
        let span = link_span(&props, &p);
        assert_eq!(span.content, "job:14445");
    }
}
