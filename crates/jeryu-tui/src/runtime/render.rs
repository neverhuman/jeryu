//! Frame composition + the `--once` render-to-string driver.
//!
//! The top-level [`draw`] composes the chrome (header / content / status strip)
//! around the active lens. [`render_once`] renders a single deterministic frame
//! into an in-memory `TestBackend` and returns the flattened cell text — this is
//! the tuiwright-style snapshot surface that proves the read-model→TUI rewire
//! without a real terminal.

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout};

use crate::app::{ActiveTab, App};
use crate::lenses::{self, LensId};
use crate::theme::badges::FreshnessBadge;
use crate::theme::palette::Palette;
use crate::widgets::header::{self, HeaderProps, StreamMode};
use crate::widgets::status_strip::{self, FrameMetrics, KeyHint, StatusStripProps};

const BRAND: &str = "jeryu Flight Deck";

/// Static header tab labels in [`ActiveTab::ALL`] order.
fn tab_labels() -> Vec<&'static str> {
    ActiveTab::ALL.iter().map(|t| t.label()).collect()
}

/// Compose one full Flight Deck frame: header strip, the active lens content,
/// and the bottom status strip.
pub fn draw(f: &mut Frame, app: &App, stream_mode: StreamMode) {
    let palette = Palette::dark();
    let area = f.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // header
            Constraint::Min(0),    // lens content
            Constraint::Length(2), // status strip
        ])
        .split(area);

    // Header
    let labels = tab_labels();
    let worst = freshness_badge(app);
    let header_props = HeaderProps {
        brand: BRAND,
        tab_labels: &labels,
        active_tab: app.active_tab.index(),
        worst_freshness: worst,
        stream_mode,
    };
    header::render(f, &header_props, rows[0], &palette);

    // Content — the active lens projected from the read model.
    let lens = LensId::for_tab(app.active_tab);
    lenses::draw_lens(f, lens, &app.model, rows[1]);

    // Status strip
    let hints = [
        KeyHint {
            key: "Tab",
            desc: "next pane",
        },
        KeyHint {
            key: "0-9",
            desc: "tabs",
        },
        KeyHint {
            key: ":",
            desc: "cmd",
        },
        KeyHint {
            key: "?",
            desc: "help",
        },
        KeyHint {
            key: "q",
            desc: "quit",
        },
    ];
    let strip = StatusStripProps {
        hints: &hints,
        metrics: FrameMetrics {
            frame_ms: 0,
            fps: 60,
            dropped: 0,
        },
    };
    status_strip::render(f, &strip, rows[2], &palette);
}

/// Map the read model's overall freshness to a header chip.
fn freshness_badge(app: &App) -> FreshnessBadge {
    if app.model.freshness.overall_stale {
        FreshnessBadge::Expired
    } else {
        FreshnessBadge::Fresh
    }
}

/// Render one deterministic frame for `tab` into an in-memory backend and
/// return the flattened cell text. Used by `--once` and snapshot tests.
pub fn render_once(app: &App, width: u16, height: u16, stream_mode: StreamMode) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("construct test terminal");
    terminal
        .draw(|f| draw(f, app, stream_mode))
        .expect("render frame");
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::{TuiReadModel, sample_read_model};

    fn app_for(tab: ActiveTab, model: TuiReadModel) -> App {
        let mut app = App::new_render_only(model);
        app.set_tab(tab);
        app
    }

    #[test]
    fn once_renders_header_brand_and_status_strip() {
        let app = app_for(ActiveTab::Mission, TuiReadModel::default());
        let ink = render_once(&app, 120, 40, StreamMode::Fixture);
        assert!(ink.contains("jeryu"));
        assert!(ink.contains("Mission"));
        assert!(ink.contains("FIXTURE"));
        assert!(ink.contains("quit"));
    }

    #[test]
    fn once_renders_implemented_lens_content() {
        let app = app_for(ActiveTab::Mission, sample_read_model());
        let ink = render_once(&app, 120, 40, StreamMode::Live);
        assert!(ink.contains("Posture"));
        assert!(ink.contains("LIVE"));
    }

    #[test]
    fn once_renders_live_summary_for_cache_tab() {
        let app = app_for(ActiveTab::Cache, TuiReadModel::default());
        let ink = render_once(&app, 120, 40, StreamMode::Poll);
        assert!(ink.contains("Hit ratio"));
    }

    #[test]
    fn once_renders_ported_agents_lens() {
        let app = app_for(ActiveTab::Agents, sample_read_model());
        let ink = render_once(&app, 120, 40, StreamMode::Live);
        assert!(ink.contains("Agents"));
        assert!(ink.contains("Lifecycle"));
    }

    #[test]
    fn degraded_freshness_watermark_renders_expired_chip() {
        // A degraded read-model freshness watermark maps to the EXPIRED
        // freshness chip in the header; a current watermark shows FRESH.
        let mut model = TuiReadModel::default();
        model.freshness.overall_stale = true;
        let degraded = app_for(ActiveTab::Mission, model);
        assert_eq!(freshness_badge(&degraded), FreshnessBadge::Expired);
        let degraded_ink = render_once(&degraded, 120, 40, StreamMode::Fixture);
        assert!(
            degraded_ink.contains(FreshnessBadge::Expired.label()),
            "expired chip label not rendered"
        );

        let current = app_for(ActiveTab::Mission, TuiReadModel::default());
        assert_eq!(freshness_badge(&current), FreshnessBadge::Fresh);
        assert!(
            render_once(&current, 120, 40, StreamMode::Fixture)
                .contains(FreshnessBadge::Fresh.label())
        );
    }
}
