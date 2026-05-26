//! Owner: Interactive TUI subsystem - semantic theme system
//! Proof: `cargo test -p jeryu --lib tui::theme`
//! Invariants: Theme and badge rendering are deterministic and terminal-cap aware.

pub mod badges;
pub mod glyphs;
pub mod palette;
pub mod terminal_caps;

pub use badges::{
    Badge, ProofConfidence, cache_hit_badge, cache_taint_badge, flake_badge, freshness_badge,
    proof_confidence_badge, risk_badge, status_badge, stream_mode_badge,
};
pub use glyphs::GlyphSet;
pub use palette::Theme;
pub use terminal_caps::TerminalCaps;

#[cfg(test)]
mod tests;
