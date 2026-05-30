//! `jeryu-tui` — the ratatui Flight Deck for the jeryu control plane.
//!
//! This crate is the TUI runtime skeleton + a subset of the Flight Deck lenses,
//! ported to project **purely** from the provider-neutral
//! [`TuiReadModel`](jeryu_readmodel::TuiReadModel) contract. It carries no
//! engine dependency: a future `jeryu-*` core fills the read model behind the
//! [`ReadModelSource`](jeryu_readmodel::seam) seam, and the TUI only ever
//! consumes the contract.
//!
//! ## What is ported here
//! - **Runtime skeleton**: render-only [`App`](app::App) state, the
//!   [`ActiveTab`](app::ActiveTab) set, the keyboard event-loop scaffold
//!   ([`runtime::input`]), and the frame composition driver
//!   ([`runtime::render`]).
//! - **Theme**: [`Palette`](theme::palette::Palette), [`Glyph`](theme::glyphs),
//!   [`TerminalCaps`](theme::terminal_caps), freshness/proof/risk
//!   [`badges`](theme::badges), and the [`ThemeBundle`](theme::ThemeBundle).
//! - **Focus state machine**: [`PaneId`](focus::PaneId) /
//!   [`FocusState`](focus::FocusState) + pane chrome helpers.
//! - **Reusable widgets**: header strip, bottom status strip, virtualized table.
//! - **3 lenses** wired end-to-end against the read model: `mission`, `queue`,
//!   `repos`. The remaining 15 lenses are enumerated in [`LensId`](lenses::LensId)
//!   and draw a stable placeholder until ported.
//!
//! ## Testing surface
//! [`run_tui_once`] renders a single deterministic frame for a tab into an
//! in-memory backend and returns the flattened cell text (tuiwright-style
//! snapshot). No real terminal is required.

pub mod app;
pub mod focus;
pub mod lenses;
pub mod runtime;
pub mod theme;
pub mod widgets;

pub use app::{ActiveTab, App};
pub use lenses::LensId;
pub use runtime::render::{draw, render_once};
pub use widgets::header::StreamMode;

use jeryu_readmodel::TuiReadModel;

/// Parse a capture/once tab name (CLI `--tab` value) into an [`ActiveTab`].
///
/// Accepts both the lens route names and the tab labels (case-insensitive). The
/// default product tab is `jobs`.
pub fn parse_capture_tab(tab: &str) -> Option<ActiveTab> {
    let t = tab.trim().to_ascii_lowercase();
    let t = if t.is_empty() { "jobs".to_string() } else { t };
    let resolved = match t.as_str() {
        "workflow" => ActiveTab::Workflow,
        "mission" => ActiveTab::Mission,
        "release" => ActiveTab::Release,
        "approvals" => ActiveTab::Approvals,
        "jobs" => ActiveTab::Jobs,
        "agents" => ActiveTab::Agents,
        "tests" | "vti" => ActiveTab::Tests,
        "pools" | "queue" => ActiveTab::Pools,
        "cache" => ActiveTab::Cache,
        "evidence" => ActiveTab::Evidence,
        "repos" => ActiveTab::Repos,
        "bugs" => ActiveTab::Bugs,
        "llms" => ActiveTab::LLMs,
        "git" => ActiveTab::Git,
        "secrets" => ActiveTab::Secrets,
        "jankurai" => ActiveTab::Jankurai,
        "source-doctor" | "source_doctor" | "runners" => ActiveTab::Pools,
        _ => return None,
    };
    Some(resolved)
}

/// `--once` smoke render: project a frame for `tab` from `model` and return the
/// flattened cell text. The standalone analogue of the product's
/// `run_tui_once`/`smoke_render_once` (which shelled a real terminal).
pub fn run_tui_once(model: TuiReadModel, tab: &str) -> Result<String, String> {
    let active_tab = parse_capture_tab(tab).ok_or_else(|| format!("unknown tui tab: {tab:?}"))?;
    let mut app = App::new_render_only(model);
    app.set_tab(active_tab);
    Ok(render_once(&app, 120, 40, StreamMode::Fixture))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::sample_read_model;

    #[test]
    fn parse_capture_tab_resolves_known_tabs() {
        assert_eq!(parse_capture_tab("mission"), Some(ActiveTab::Mission));
        assert_eq!(parse_capture_tab("REPOS"), Some(ActiveTab::Repos));
        assert_eq!(parse_capture_tab("queue"), Some(ActiveTab::Pools));
        assert_eq!(parse_capture_tab(""), Some(ActiveTab::Jobs));
        assert_eq!(parse_capture_tab("does-not-exist"), None);
    }

    #[test]
    fn run_tui_once_renders_known_tab() {
        let ink = run_tui_once(sample_read_model(), "mission").unwrap();
        assert!(ink.contains("Posture"));
        assert!(ink.contains("FIXTURE"));
    }

    #[test]
    fn run_tui_once_rejects_unknown_tab() {
        assert!(run_tui_once(TuiReadModel::default(), "bogus").is_err());
    }
}
