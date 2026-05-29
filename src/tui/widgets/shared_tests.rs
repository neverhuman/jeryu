use ratatui::{buffer::Buffer, widgets::Widget};

use super::shared::{CanonicalSize, KeyValueRows, Panel};
use crate::tui::theme::Theme;
use crate::tui::widgets::status_badge::badge_for_status;

fn render_text(widget: impl Widget, size: CanonicalSize) -> String {
    let area = size.area();
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn panel_renders_at_canonical_sizes() {
    let theme = Theme::dark();
    for size in [
        CanonicalSize::Compact,
        CanonicalSize::Standard,
        CanonicalSize::Wide,
    ] {
        let badge = badge_for_status("success", &theme);
        let text = render_text(
            Panel::new("Mission", &theme)
                .active(true, &theme)
                .badge(badge)
                .line("  READY")
                .line("  proof: measured"),
            size,
        );
        assert!(text.contains("Mission"));
        assert!(text.contains("READY"));
    }
}

#[test]
fn panel_empty_state_is_explicit() {
    let theme = Theme::dark();
    let text = render_text(
        Panel::new("Evidence", &theme).empty("No evidence"),
        CanonicalSize::Compact,
    );
    assert!(text.contains("Evidence"));
    assert!(text.contains("No evidence"));
}

#[test]
fn key_value_rows_render_labels_and_values() {
    let theme = Theme::dark();
    let badge = badge_for_status("running", &theme);
    let text = render_text(
        KeyValueRows::new("Source", &theme)
            .badge(badge)
            .row("state", "live")
            .row("cursor", "42"),
        CanonicalSize::Standard,
    );

    assert!(text.contains("Source"));
    assert!(text.contains("state"));
    assert!(text.contains("live"));
    assert!(text.contains("cursor"));
    assert!(text.contains("42"));
}
