//! Owner: Interactive TUI subsystem — reusable widget library
//! Proof: `cargo nextest run -p jeryu -- tui::widgets`
//! Invariants: Widgets are pure rendering functions; they never mutate control-plane state.

pub mod action_dispatch;
pub mod agent_fleet;
pub mod attention;
pub mod inspector;
pub mod mission;
pub mod mission_shared;
pub mod sparkline;
pub mod status_badge;
pub mod timeline;
pub mod vti_proof;

// ── U13 shared widgets baseline ─────────────────────────────────────────
// Each widget is a stand-alone file with a `render(...)` entry point and a
// render test at canonical 80x24 + 120x36 sizes. See plan §17 U13.
pub mod command_palette;
pub mod dag;
pub mod entity_link;
pub mod event_tape;
pub mod forms;
pub mod freshness_chip;
pub mod header;
pub mod heatmap;
pub mod help;
pub mod inspector_card;
pub mod log_viewer;
pub mod modal;
pub mod progress_bar;
pub mod proof_chip;
pub mod status_strip;
pub mod tabs;
pub mod virtual_table;

/// Shared text truncation for widget labels.
/// Truncates to `max` characters with a trailing ellipsis if needed.
pub fn truncate_label(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        s.to_string()
    } else if max > 1 {
        format!("{}…", &s[..max - 1])
    } else {
        s[..max].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_label_preserves_short() {
        assert_eq!(truncate_label("hello", 10), "hello");
    }

    #[test]
    fn truncate_label_truncates_long() {
        assert_eq!(truncate_label("hello world", 6), "hello…");
    }

    #[test]
    fn truncate_label_zero_max() {
        assert_eq!(truncate_label("test", 0), "");
    }
}
