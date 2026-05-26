//! Owner: Interactive TUI subsystem — reusable widget library
//! Proof: `cargo nextest run -p jeryu -- tui::widgets`
//! Invariants: Widgets are pure rendering functions; they never mutate control-plane state.

pub mod action_dispatch;
pub mod agent_fleet;
pub mod attention;
pub mod inspector;
pub mod mission;
pub mod mission_shared;
pub mod shared;
pub mod sparkline;
pub mod status_badge;
pub mod timeline;
pub mod vti_proof;

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

#[cfg(test)]
mod shared_tests;
