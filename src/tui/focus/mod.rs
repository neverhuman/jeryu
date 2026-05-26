//! Focus state, pane identity, and pane chrome helpers.

mod chrome;
mod map;
mod pane;
mod state;

pub use crate::tui::nav::NavDirection;
pub use chrome::{
    PaneChrome, border_color, border_style, esc_label, is_active, pane_chrome,
    register_drill_esc_hotspot, register_esc_hotspot, register_pane, should_show_drill_esc,
    should_show_esc, title_with_esc,
};
pub use map::{FocusMap, FocusPane};
pub use pane::PaneId;
pub use state::FocusState;

#[cfg(test)]
mod tests;
