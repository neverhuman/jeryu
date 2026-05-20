//! Owner: Interactive TUI subsystem - Tuiwright black-box input smoke tests
//! Proof: `TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1`
//! Invariants: PNG capture is the render oracle; PTY sessions are reserved for input routing.

use image::RgbImage;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
use tuiwright::{Key, Page, ScreenSnapshot, SpawnConfig};

const CAPTURE_COLS: u16 = 120;
const CAPTURE_ROWS: u16 = 36;
const CELL_W: u32 = 8;
const CELL_H: u32 = 12;
const XTERM_YELLOW: (u8, u8, u8) = (0xcd, 0xcd, 0x00);

/// Locate the `jeryu` binary built by cargo.
fn jeryu_bin() -> String {
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

fn capture_tui(tab: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(format!("target/tuiwright/capture-{tab}.png"));
    std::fs::create_dir_all("target/tuiwright")?;

    let output = Command::new(jeryu_bin())
        .arg("tui")
        .arg("--capture")
        .arg("--tab")
        .arg(tab)
        .arg("--output")
        .arg(&path)
        .arg("--width")
        .arg(CAPTURE_COLS.to_string())
        .arg("--height")
        .arg(CAPTURE_ROWS.to_string())
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("JERYU_DATABASE_URL", "redline::memory:")
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

fn read_png(path: &Path) -> anyhow::Result<RgbImage> {
    Ok(image::open(path)?.to_rgb8())
}

fn assert_png_shape_and_ink(path: &Path, image: &RgbImage) {
    assert_eq!(
        image.dimensions(),
        (
            u32::from(CAPTURE_COLS) * CELL_W,
            u32::from(CAPTURE_ROWS) * CELL_H
        ),
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

fn assert_cell_region_has_ink(
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

fn assert_main_layout_regions(tab: &str, image: &RgbImage) {
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

fn spawn_interactive_tui(tab: &str) -> anyhow::Result<Page> {
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--demo")
            .arg("--tab")
            .arg(tab)
            .size(160, 40)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("NO_COLOR", "")
            .env("JERYU_TUI_WORKFLOW_INSPECT_OPEN", "1")
            .timeout(Duration::from_secs(8)),
    )?;
    std::thread::sleep(Duration::from_millis(2000));
    Ok(page)
}

fn screen_text(page: &Page) -> String {
    page.screen().plain_text()
}

fn is_yellow_cell(cell: &tuiwright::CellSnapshot) -> bool {
    (cell.fg.r, cell.fg.g, cell.fg.b) == XTERM_YELLOW
}

fn title_row_yellow_cell_count(screen: &ScreenSnapshot, title: &str) -> Option<usize> {
    let title_match = screen.find_text(title).into_iter().next()?;
    Some(
        (0..screen.cols)
            .filter_map(|col| screen.cell(title_match.row, col))
            .filter(|cell| is_yellow_cell(cell))
            .count(),
    )
}

fn title_row_fg_summary(screen: &ScreenSnapshot, title: &str) -> String {
    let Some(title_match) = screen.find_text(title).into_iter().next() else {
        return "title not found".into();
    };
    let mut counts = std::collections::BTreeMap::<(u8, u8, u8), usize>::new();
    for col in 0..screen.cols {
        if let Some(cell) = screen.cell(title_match.row, col) {
            *counts.entry((cell.fg.r, cell.fg.g, cell.fg.b)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|((r, g, b), count)| format!("#{r:02x}{g:02x}{b:02x}:{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn assert_focused_title_row(screen: &ScreenSnapshot, title: &str) -> anyhow::Result<()> {
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

fn wait_for_focused_title(page: &Page, title: &str) -> anyhow::Result<()> {
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

#[test]
fn capture_path_renders_all_primary_tabs() -> anyhow::Result<()> {
    for tab in [
        "workflow",
        "mission",
        "release",
        "approvals",
        "jobs",
        "agents",
        "tests",
        "pools",
        "cache",
        "evidence",
        "bugs",
        "secrets",
        "llms",
        "git",
    ] {
        let path = capture_tui(tab)?;
        let image = read_png(&path)?;
        assert_png_shape_and_ink(&path, &image);
        assert_main_layout_regions(tab, &image);
    }
    Ok(())
}

#[test]
fn tab_always_cycles_main_tabs_from_workflow() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("workflow")?;

    page.wait_for_text("#1842", Duration::from_secs(5))?;
    page.press(Key::Tab)?;
    page.wait_for_text("Mission Control", Duration::from_secs(5))?;

    for _ in 0..13 {
        page.press(Key::Tab)?;
    }

    page.wait_for_text("Pre-merge CI", Duration::from_secs(5))?;
    let text = screen_text(&page);
    assert!(text.contains("#1842"));
    Ok(())
}

#[test]
fn keyboard_macro_focuses_activity_log_and_drills_down() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("jobs")?;

    page.wait_for_text("Activity / Logs", Duration::from_secs(5))?;
    wait_for_focused_title(&page, "Live Runner Feed")?;

    page.press(Key::Left)?;
    wait_for_focused_title(&page, "Live Runner Feed")?;

    page.press(Key::Down)?;
    wait_for_focused_title(&page, "Activity / Logs")?;

    page.press(Key::Left)?;
    wait_for_focused_title(&page, "Activity / Logs")?;

    page.press(Key::Right)?;
    let before_enter = page.screen();
    assert_focused_title_row(&before_enter, "Activity / Logs")?;

    page.press(Key::Enter)?;
    page.wait_for_text("[esc]", Duration::from_secs(5))?;
    let fullscreen = page.screen();
    let fullscreen_text = fullscreen.plain_text();
    assert!(fullscreen_text.contains("Activity / Logs"));
    assert!(
        fullscreen_text.contains("Job") || fullscreen_text.contains("Jobs"),
        "fullscreen activity/log content should remain visible\n\nscreen:\n{fullscreen_text}"
    );
    assert!(
        !fullscreen_text.contains("Pipeline Progress"),
        "fullscreen activity/log should hide the jobs pipeline pane\n\nscreen:\n{fullscreen_text}"
    );
    assert!(
        !fullscreen_text.contains("Live Runner Feed"),
        "fullscreen activity/log should hide the live runner feed\n\nscreen:\n{fullscreen_text}"
    );
    assert!(
        !fullscreen_text.contains("Job Matrix"),
        "fullscreen activity/log should hide the job matrix\n\nscreen:\n{fullscreen_text}"
    );

    page.press(Key::Esc)?;
    page.wait_for_text("Pipeline Progress", Duration::from_secs(5))?;
    let restored = page.screen();
    let restored_text = restored.plain_text();
    assert!(
        !restored_text
            .lines()
            .nth(3)
            .unwrap_or_default()
            .contains("[esc]"),
        "fullscreen activity/log title should be gone after Esc\n\nscreen:\n{restored_text}"
    );

    page.press(Key::Up)?;
    wait_for_focused_title(&page, "Inspector")?;
    Ok(())
}

#[test]
fn activity_log_enter_expands_and_esc_restores() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("jobs")?;

    page.wait_for_text("Activity / Logs", Duration::from_secs(5))?;
    let locator = page.get_by_text("Activity / Logs");
    let match_ = locator
        .resolve_first(&page.screen())
        .expect("expected activity log pane to be visible");
    let (col, row) = match_.center();
    page.click_cell(col, row)?;

    page.press(Key::Enter)?;
    page.wait_for_text("[esc]", Duration::from_secs(5))?;
    page.expect_screen().not_to_contain_text("Pipeline")?;

    page.press(Key::Esc)?;
    page.wait_for_text("Pipeline", Duration::from_secs(5))?;
    Ok(())
}

#[test]
fn esc_badge_click_exits_entered_pane() -> anyhow::Result<()> {
    let page = spawn_interactive_tui("jobs")?;

    page.wait_for_text("Activity / Logs", Duration::from_secs(5))?;
    let locator = page.get_by_text("Activity / Logs");
    let match_ = locator
        .resolve_first(&page.screen())
        .expect("expected activity log pane to be visible");
    let (col, row) = match_.center();
    page.click_cell(col, row)?;

    page.press(Key::Enter)?;
    page.wait_for_text("[esc]", Duration::from_secs(5))?;

    let esc = page.get_by_text("[esc]");
    let esc_match = esc
        .resolve_first(&page.screen())
        .expect("expected esc badge in fullscreen activity log");
    let (esc_col, esc_row) = esc_match.center();
    page.click_cell(esc_col, esc_row)?;

    page.wait_for_text("Pipeline", Duration::from_secs(5))?;
    Ok(())
}
