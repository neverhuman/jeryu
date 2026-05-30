//! Owner: Tuiwright test suite - shared helpers
//! Proof: helpers compile under `cargo nextest run --test tuiwright`
//! Invariants: every helper preserved verbatim from the pre-split tests/tui_tuiwright.rs

use image::RgbImage;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tuiwright::{Page, ScreenSnapshot, SpawnConfig};

pub const CAPTURE_COLS: u16 = 120;
pub const CAPTURE_ROWS: u16 = 36;
pub const CELL_W: u32 = 8;
pub const CELL_H: u32 = 12;
pub const XTERM_YELLOW: (u8, u8, u8) = (0xcd, 0xcd, 0x00);
static TUIWRIGHT_LOCK: Mutex<()> = Mutex::new(());

pub fn tuiwright_lock() -> MutexGuard<'static, ()> {
    TUIWRIGHT_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

/// Locate the `jeryu` binary built by cargo.
pub fn jeryu_bin() -> String {
    // When run via `cargo test`, CARGO_BIN_EXE_jeryu is set automatically.
    match std::env::var("CARGO_BIN_EXE_jeryu") {
        Ok(path) => path,
        Err(_) => {
            // Fallback: look in target/debug
            let manifest = std::env::var("CARGO_MANIFEST_DIR")
                .expect("CARGO_MANIFEST_DIR must be set by cargo");
            format!("{manifest}/target/debug/jeryu")
        }
    }
}

pub fn capture_tui(tab: &str) -> anyhow::Result<PathBuf> {
    capture_tui_size(tab, CAPTURE_COLS, CAPTURE_ROWS)
}

pub fn capture_tui_size(tab: &str, cols: u16, rows: u16) -> anyhow::Result<PathBuf> {
    let path = if cols == CAPTURE_COLS && rows == CAPTURE_ROWS {
        PathBuf::from(format!("target/tuiwright/capture-{tab}.png"))
    } else {
        PathBuf::from(format!("target/tuiwright/capture-{tab}-{cols}x{rows}.png"))
    };
    std::fs::create_dir_all("target/tuiwright")?;

    let output = Command::new(jeryu_bin())
        .arg("tui")
        .arg("--capture")
        .arg("--tab")
        .arg(tab)
        .arg("--output")
        .arg(&path)
        .arg("--width")
        .arg(cols.to_string())
        .arg("--height")
        .arg(rows.to_string())
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url())
        .output()?;

    anyhow::ensure!(
        output.status.success(),
        "capture failed for {tab} with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(path)
}

pub fn read_png(path: &Path) -> anyhow::Result<RgbImage> {
    Ok(image::open(path)?.to_rgb8())
}

pub fn assert_png_shape_and_ink(path: &Path, image: &RgbImage) {
    assert_png_shape_and_ink_size(path, image, CAPTURE_COLS, CAPTURE_ROWS);
}

pub fn assert_png_shape_and_ink_size(path: &Path, image: &RgbImage, cols: u16, rows: u16) {
    assert_eq!(
        image.dimensions(),
        (u32::from(cols) * CELL_W, u32::from(rows) * CELL_H),
        "unexpected PNG dimensions for {}",
        path.display()
    );
    let bg = image.get_pixel(0, 0).0;
    let ink = image.pixels().filter(|pixel| pixel.0 != bg).count();
    assert!(
        ink > 1_000,
        "capture should contain rendered terminal ink; only {ink} non-background pixels in {}",
        path.display()
    );
}

pub fn assert_cell_region_has_ink(
    image: &RgbImage,
    bg: [u8; 3],
    label: &str,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
) {
    let x0 = u32::from(x) * CELL_W;
    let y0 = u32::from(y) * CELL_H;
    let x1 = (u32::from(x + width) * CELL_W).min(image.width());
    let y1 = (u32::from(y + height) * CELL_H).min(image.height());
    let mut ink = 0usize;
    for py in y0..y1 {
        for px in x0..x1 {
            if image.get_pixel(px, py).0 != bg {
                ink += 1;
            }
        }
    }
    assert!(ink > 120, "{label} region should contain rendered ink");
}

pub fn assert_main_layout_regions(tab: &str, image: &RgbImage) {
    let bg = image.get_pixel(0, 0).0;
    assert_cell_region_has_ink(image, bg, &format!("{tab} header"), 0, 0, CAPTURE_COLS, 3);
    assert_cell_region_has_ink(
        image,
        bg,
        &format!("{tab} content"),
        0,
        3,
        CAPTURE_COLS,
        CAPTURE_ROWS - 11,
    );
    assert_cell_region_has_ink(
        image,
        bg,
        &format!("{tab} activity/log"),
        0,
        CAPTURE_ROWS - 8,
        CAPTURE_COLS,
        7,
    );
    assert_cell_region_has_ink(
        image,
        bg,
        &format!("{tab} footer"),
        0,
        CAPTURE_ROWS - 1,
        CAPTURE_COLS,
        1,
    );
}

pub fn spawn_interactive_tui(tab: &str) -> anyhow::Result<Page> {
    spawn_interactive_tui_size(tab, 160, 40)
}

pub fn spawn_interactive_tui_size(tab: &str, cols: u16, rows: u16) -> anyhow::Result<Page> {
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--demo")
            .arg("--tab")
            .arg(tab)
            .size(cols, rows)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("NO_COLOR", "")
            .env("JERYU_TUI_WORKFLOW_INSPECT_OPEN", "1")
            .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url())
            .timeout(Duration::from_secs(8)),
    )?;
    std::thread::sleep(Duration::from_millis(2000));
    Ok(page)
}

pub fn screen_text(page: &Page) -> String {
    page.screen().plain_text()
}

pub fn is_yellow_cell(cell: &tuiwright::CellSnapshot) -> bool {
    (cell.fg.r, cell.fg.g, cell.fg.b) == XTERM_YELLOW
}

pub fn find_text_cell_region(screen: &ScreenSnapshot, needle: &str) -> Option<(u16, u16, u16)> {
    for row in 0..screen.rows {
        let mut line = String::new();
        let mut byte_to_col = Vec::<(usize, u16)>::new();
        for col in 0..screen.cols {
            let cell = screen.cell(row, col)?;
            if cell.wide_continuation {
                continue;
            }
            byte_to_col.push((line.len(), col));
            if cell.text.is_empty() {
                line.push(' ');
            } else {
                line.push_str(&cell.text);
            }
        }
        if let Some(byte_pos) = line.find(needle) {
            let col = byte_to_col
                .iter()
                .rev()
                .find(|(offset, _)| *offset <= byte_pos)
                .map(|(_, col)| *col)?;
            return Some((row, col, needle.chars().count() as u16));
        }
    }
    None
}

pub fn title_row_yellow_cell_count(screen: &ScreenSnapshot, title: &str) -> Option<usize> {
    let (row, col, width) = find_text_cell_region(screen, title)?;
    let start = col.saturating_sub(2);
    let end = col.saturating_add(width).saturating_add(8).min(screen.cols);
    Some(
        (start..end)
            .filter_map(|col| screen.cell(row, col))
            .filter(|cell| is_yellow_cell(cell))
            .count(),
    )
}

pub fn title_row_fg_summary(screen: &ScreenSnapshot, title: &str) -> String {
    let Some((row, col, width)) = find_text_cell_region(screen, title) else {
        return "title not found".into();
    };
    let start = col.saturating_sub(2);
    let end = col.saturating_add(width).saturating_add(8).min(screen.cols);
    let mut counts = std::collections::BTreeMap::<(u8, u8, u8), usize>::new();
    for col in start..end {
        if let Some(cell) = screen.cell(row, col) {
            *counts.entry((cell.fg.r, cell.fg.g, cell.fg.b)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|((r, g, b), count)| format!("#{r:02x}{g:02x}{b:02x}:{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn assert_focused_title_row(screen: &ScreenSnapshot, title: &str) -> anyhow::Result<()> {
    let yellow_cells = title_row_yellow_cell_count(screen, title).ok_or_else(|| {
        anyhow::anyhow!(
            "expected pane title {title:?} to be visible\n\nscreen:\n{}",
            screen.plain_text()
        )
    })?;
    anyhow::ensure!(
        yellow_cells >= 8,
        "expected pane title {title:?} to have a yellow focused border/title row, found {yellow_cells} yellow cells; row colors: {}\n\nscreen:\n{}",
        title_row_fg_summary(screen, title),
        screen.plain_text()
    );
    Ok(())
}

pub fn wait_for_focused_title(page: &Page, title: &str) -> anyhow::Result<()> {
    let timeout = Duration::from_secs(5);
    let deadline = Instant::now() + timeout;
    let mut last = page.screen();
    loop {
        if title_row_yellow_cell_count(&last, title).unwrap_or(0) >= 8 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            assert_focused_title_row(&last, title)?;
        }
        std::thread::sleep(Duration::from_millis(50));
        last = page.screen();
    }
}

pub fn assert_text_absent(screen: &ScreenSnapshot, needle: &str) -> anyhow::Result<()> {
    let text = screen.plain_text();
    anyhow::ensure!(
        !text.contains(needle),
        "expected screen not to contain {needle:?}\n\nscreen:\n{text}"
    );
    Ok(())
}

pub fn wait_for_text_absent(page: &Page, needle: &str) -> anyhow::Result<()> {
    let timeout = Duration::from_secs(5);
    let deadline = Instant::now() + timeout;
    let mut last = page.screen();
    loop {
        if !last.plain_text().contains(needle) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            assert_text_absent(&last, needle)?;
        }
        std::thread::sleep(Duration::from_millis(50));
        last = page.screen();
    }
}
