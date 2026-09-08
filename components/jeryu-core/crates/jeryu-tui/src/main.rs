use std::io;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use jeryu_readmodel::{TuiReadModel, sample_read_model};
use jeryu_tui::runtime::{Flow, handle_key};
use jeryu_tui::{App, StreamMode, draw, parse_capture_tab, render_once};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

#[derive(Debug, Parser)]
#[command(name = "jeryu-tui")]
struct Cli {
    #[arg(long)]
    once: bool,
    #[arg(long, default_value = "mission")]
    tab: String,
    #[arg(long, value_enum, default_value_t = Source::Fixture)]
    source: Source,
    #[arg(long, default_value = "http://127.0.0.1:8787")]
    api_url: String,
    #[arg(long, default_value_t = 120)]
    width: u16,
    #[arg(long, default_value_t = 40)]
    height: u16,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Source {
    Fixture,
    Api,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let tab =
        parse_capture_tab(&cli.tab).ok_or_else(|| format!("unknown tui tab: {:?}", cli.tab))?;
    let model = load_model(cli.source, &cli.api_url)?;
    let mut app = App::new_render_only(model);
    app.set_tab(tab);
    let stream = match cli.source {
        Source::Fixture => StreamMode::Fixture,
        Source::Api => StreamMode::Live,
    };

    if cli.once {
        println!("{}", render_once(&app, cli.width, cli.height, stream));
    } else {
        run_interactive(&mut app, stream)?;
    }
    Ok(())
}

/// Enter the full interactive crossterm event loop.
fn run_interactive(
    app: &mut App,
    stream_mode: StreamMode,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main event loop
    let tick_rate = Duration::from_millis(250);
    let result = loop {
        terminal.draw(|f| draw(f, app, stream_mode))?;

        if event::poll(tick_rate)? {
            match event::read()? {
                // crossterm 0.29 fires Press + Release; only act on Press.
                Event::Key(key)
                    if key.kind == KeyEventKind::Press && handle_key(app, key) == Flow::Quit =>
                {
                    break Ok(());
                }
                _ => {}
            }
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn load_model(source: Source, api_url: &str) -> Result<TuiReadModel, Box<dyn std::error::Error>> {
    match source {
        Source::Fixture => Ok(sample_read_model()),
        Source::Api => {
            let response = reqwest::blocking::get(tui_bootstrap_url(api_url))?
                .error_for_status()?
                .json::<TuiReadModel>()?;
            Ok(response)
        }
    }
}

fn tui_bootstrap_url(api_url: &str) -> String {
    format!("{}/api/v1/bootstrap.tui", api_url.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_url_builder_is_stable() {
        assert_eq!(
            tui_bootstrap_url("http://127.0.0.1:8787/"),
            "http://127.0.0.1:8787/api/v1/bootstrap.tui"
        );
    }
}
