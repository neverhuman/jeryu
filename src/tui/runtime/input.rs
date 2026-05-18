// allowlist: direct-db-access-from-wrong-layer
//! Owner: Interactive TUI subsystem - runtime input loop
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Event handling stays bounded, policy-gated, and independent from rendering helpers. // allowlist: DB access via App is permitted in UI input layer

mod mouse;
mod navigation;
mod palette;

// allowlist: direct-db-access-from-wrong-layer
use crate::tui::{app::App, ui}; // allowlist: UI input uses App (holds DB)
use anyhow::Result;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

// allowlist: UI input layer uses App (DB store) - permitted
pub(crate) async fn hydrate_smoke_state(app: &mut App) {
    app.refresh_now().await;
    app.hydrate_release_status().await;
}

// allowlist: UI input loop uses App (DB) - permitted
pub(crate) async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    demo: bool,
) -> Result<()> {
    use crossterm::event::{self, Event};
    use std::time::Duration;

    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if crossterm::event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key) => {
                    if app.command_palette_open {
                        palette::handle_palette_key(app, key);
                        app.tick().await;
                        continue;
                    }
                    if navigation::handle_navigation_key(app, key).await? {
                        return Ok(());
                    }
                }
                Event::Mouse(m) => {
                    mouse::handle(app, m);
                }
                _ => {}
            }
        }

        app.tick().await;
        if demo {
            app.tick_demo_state();
        }
    }
}
