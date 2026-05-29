use image::RgbImage;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, MutexGuard},
};

pub(crate) const CAPTURE_COLS: u16 = 120;
pub(crate) const CAPTURE_ROWS: u16 = 36;
const CELL_W: u32 = 8;
const CELL_H: u32 = 12;
static TUIWRIGHT_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn tuiwright_lock() -> MutexGuard<'static, ()> {
    TUIWRIGHT_LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

pub(crate) fn capture_tui(tab: &str) -> anyhow::Result<PathBuf> {
    capture_tui_size(tab, CAPTURE_COLS, CAPTURE_ROWS)
}

pub(crate) fn capture_tui_size(tab: &str, cols: u16, rows: u16) -> anyhow::Result<PathBuf> {
    let path = if cols == CAPTURE_COLS && rows == CAPTURE_ROWS {
        PathBuf::from(format!("target/tuiwright/split-capture-{tab}.png"))
    } else {
        PathBuf::from(format!(
            "target/tuiwright/split-capture-{tab}-{cols}x{rows}.png"
        ))
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

pub(crate) fn read_png(path: &Path) -> anyhow::Result<RgbImage> {
    Ok(image::open(path)?.to_rgb8())
}

pub(crate) fn assert_png_shape_and_ink(path: &Path, image: &RgbImage) {
    assert_png_shape_and_ink_size(path, image, CAPTURE_COLS, CAPTURE_ROWS);
}

pub(crate) fn assert_png_shape_and_ink_size(path: &Path, image: &RgbImage, cols: u16, rows: u16) {
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

pub(crate) fn assert_cell_region_has_ink(
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

pub(crate) fn assert_main_layout_regions(tab: &str, image: &RgbImage) {
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

fn jeryu_bin() -> String {
    match std::env::var("CARGO_BIN_EXE_jeryu") {
        Ok(path) => path,
        Err(_) => {
            let manifest = std::env::var("CARGO_MANIFEST_DIR")
                .expect("CARGO_MANIFEST_DIR must be set by cargo");
            format!("{manifest}/target/debug/jeryu")
        }
    }
}
