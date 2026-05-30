//! Focus module root — pane identity, state stack, and pane chrome.
//!
//! Faithful port of the source focus subsystem: pane identity ([`pane`]), the
//! drill-down state machine ([`state`]), and bordered-pane chrome helpers
//! ([`chrome`]).

pub mod chrome;
pub mod pane;
pub mod state;

pub use chrome::{
    border_color, border_style, esc_label, is_active, pane_block, should_show_esc, title_with_esc,
};
pub use pane::{NavDirection, PaneId};
pub use state::FocusState;
