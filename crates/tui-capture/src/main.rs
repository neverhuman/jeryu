use anyhow::{Context, Result, bail};
use clap::Parser;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

#[path = "support.rs"]
mod support;

use support::*;

#[path = "capture_runtime.rs"]
mod capture_runtime;

#[derive(Parser, Debug)]
#[command(
    name = "tui-capture",
    about = "Capture a terminal TUI screen as a deterministic PNG"
)]
struct Args {
    #[arg(long, default_value_t = 160)]
    cols: u16,
    #[arg(long, default_value_t = 48)]
    rows: u16,
    #[arg(long, default_value = "tui.png")]
    out: PathBuf,
    #[arg(long)]
    font: Option<PathBuf>,
    #[arg(long = "font-size", default_value_t = 19.0)]
    font_px: f32,
    #[arg(long = "cell-w", default_value_t = 12)]
    cell_w: u32,
    #[arg(long = "cell-h", default_value_t = 23)]
    cell_h: u32,
    #[arg(long, default_value_t = 14)]
    padding: u32,
    #[arg(long, default_value = "#17212b")]
    bg: String,
    #[arg(long, default_value = "#f4fbff")]
    fg: String,
    #[arg(long, default_value_t = 1.35)]
    brighten: f32,
    #[arg(long = "respect-dim", default_value_t = false)]
    respect_dim: bool,
    #[arg(long = "min-wait-ms", default_value_t = 1200)]
    min_wait_ms: u64,
    #[arg(long = "max-wait-ms", default_value_t = 8000)]
    max_wait_ms: u64,
    #[arg(long = "quiet-ms", default_value_t = 300)]
    quiet_ms: u64,
    #[arg(long = "send-after-ms", default_value_t = 0)]
    send_after_ms: u64,
    #[arg(long = "send", action = clap::ArgAction::Append)]
    send: Vec<String>,
    #[arg(long = "dump-text")]
    dump_text: Option<PathBuf>,
    #[arg(long = "ready-file")]
    ready_file: Option<PathBuf>,
    #[arg(
        last = true,
        required = true,
        value_parser = clap::builder::OsStringValueParser::new()
    )]
    cmd: Vec<OsString>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.cmd.is_empty() {
        bail!("missing command; use: tui-capture [opts] -- ./your-tui");
    }

    let font_path = match args.font.clone() {
        Some(path) => path,
        None => find_font()
            .context("could not find a monospace font; pass --font /path/to/DejaVuSansMono.ttf")?,
    };
    let font_bytes = fs::read(&font_path)
        .with_context(|| format!("failed to read font {}", font_path.display()))?;
    let font = rusttype::Font::try_from_vec(font_bytes)
        .with_context(|| format!("failed to parse font {}", font_path.display()))?;
    validate_required_glyphs(&font)?;

    let cfg = RenderConfig {
        scale: rusttype::Scale::uniform(args.font_px),
        cell_w: args.cell_w,
        cell_h: args.cell_h,
        padding: args.padding,
        default_bg: parse_hex_rgb(&args.bg)?,
        default_fg: parse_hex_rgb(&args.fg)?,
        brighten: args.brighten,
        respect_dim: args.respect_dim,
    };

    eprintln!(
        "capture: {}x{} cells, {}x{} px cells, font={}, out={}",
        args.cols,
        args.rows,
        cfg.cell_w,
        cfg.cell_h,
        font_path.display(),
        args.out.display()
    );

    let screen = capture_runtime::run_and_capture(&args, cfg.cell_w, cfg.cell_h)?;
    if let Some(path) = &args.dump_text {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, screen.contents())
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    let img = render_screen(&screen, &font, &cfg);
    if let Some(parent) = args.out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    img.save(&args.out)
        .with_context(|| format!("failed to save {}", args.out.display()))?;
    eprintln!(
        "wrote {} ({}x{} px)",
        args.out.display(),
        img.width(),
        img.height()
    );
    Ok(())
}
