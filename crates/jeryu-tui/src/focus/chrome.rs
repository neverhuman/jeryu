//! Pane chrome helpers — bordered-pane styling driven by focus state.
//!
//! Pure rendering helpers consumed by every widget that draws a bordered pane.
//! Active panes get a highlighted border; an `Esc` affordance label is shown
//! when the pane is drilled into.

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders};

use super::pane::PaneId;
use super::state::FocusState;
use crate::theme::palette::Palette;

/// Is `pane` the active focus target?
pub fn is_active(focus: &FocusState, pane: PaneId) -> bool {
    focus.active == pane
}

/// Should the pane show a back/`Esc` affordance? True when the operator has
/// drilled into this pane (stack non-empty or fullscreen on this pane).
pub fn should_show_esc(focus: &FocusState, pane: PaneId) -> bool {
    focus.active == pane && (focus.fullscreen == Some(pane) || !focus.stack.is_empty())
}

/// Border color: active panes use `running`, inactive panes use `stale`.
pub fn border_color(focus: &FocusState, pane: PaneId, palette: &Palette) -> ratatui::style::Color {
    if is_active(focus, pane) {
        palette.running
    } else {
        palette.stale
    }
}

/// Border style for a pane given the current focus.
pub fn border_style(focus: &FocusState, pane: PaneId, palette: &Palette) -> Style {
    let base = Style::default().fg(border_color(focus, pane, palette));
    if is_active(focus, pane) {
        base.add_modifier(Modifier::BOLD)
    } else {
        base
    }
}

/// The `Esc` affordance label (empty when the pane shows no back hint).
pub fn esc_label(focus: &FocusState, pane: PaneId) -> &'static str {
    if should_show_esc(focus, pane) {
        " Esc ◂ back "
    } else {
        ""
    }
}

/// Compose a pane title with its optional `Esc` affordance suffix.
pub fn title_with_esc(focus: &FocusState, pane: PaneId) -> String {
    let label = pane.label();
    let esc = esc_label(focus, pane);
    if esc.is_empty() {
        format!(" {label} ")
    } else {
        format!(" {label} {esc}")
    }
}

/// Build a bordered [`Block`] for a pane, styled by focus + carrying the title.
pub fn pane_block<'a>(focus: &FocusState, pane: PaneId, palette: &Palette) -> Block<'a> {
    let style = border_style(focus, pane, palette);
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(title_with_esc(focus, pane), style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ActiveTab;

    #[test]
    fn active_pane_uses_running_border() {
        let p = Palette::dark();
        let focus = FocusState::for_tab(ActiveTab::Mission);
        assert_eq!(
            border_color(&focus, PaneId::MissionTopSignal, &p),
            p.running
        );
        assert_eq!(border_color(&focus, PaneId::MissionMetrics, &p), p.stale);
    }

    #[test]
    fn esc_label_appears_only_when_drilled() {
        let mut focus = FocusState::for_tab(ActiveTab::Mission);
        assert_eq!(esc_label(&focus, PaneId::MissionTopSignal), "");
        focus.push();
        focus.active = PaneId::MissionMetrics;
        assert!(!esc_label(&focus, PaneId::MissionMetrics).is_empty());
    }

    #[test]
    fn title_includes_label() {
        let focus = FocusState::for_tab(ActiveTab::Mission);
        assert!(title_with_esc(&focus, PaneId::MissionTopSignal).contains("Top Signal"));
    }
}
