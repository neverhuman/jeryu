use std::time::Duration;
use tuiwright::{Key, Page, SpawnConfig};

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

#[test]
#[ignore] // Run manually or in CI via `cargo test --test tui_recording`
fn tui_demo_recording() -> anyhow::Result<()> {
    std::fs::create_dir_all("target/ci-screenshots")?;
    let output = std::env::var("JERYU_TUI_RECORDING_OUT")
        .unwrap_or_else(|_| "target/ci-screenshots/tui-demo.gif".to_string());
    let frame_dir = std::path::PathBuf::from("target/ci-screenshots/tui-demo-frames");
    std::fs::create_dir_all(&frame_dir)?;

    let bin = jeryu_bin();
    let config = SpawnConfig::new(&bin)
        .args(["tui", "--demo", "--tab", "workflow"])
        .size(160, 44)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("JERYU_TUI_WORKFLOW_INSPECT_OPEN", "1")
        .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url());

    let page = Page::spawn(config)?;

    // Wait for initial synthetic activity to render.
    std::thread::sleep(Duration::from_millis(1200));

    let mut frames = Vec::new();

    // --- Workflow: Macro box arrow navigation showcase ---
    // Start on PRs (default focus).
    capture_frame(&page, &frame_dir, &mut frames, "workflow-prs")?;

    // Arrow Down → Canvas box.
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(400));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-canvas")?;

    // Arrow Left → Phase box.
    page.press(Key::Left)?;
    std::thread::sleep(Duration::from_millis(400));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-phase")?;

    // Arrow Right back → Canvas, then Right → Map box.
    page.press(Key::Right)?;
    std::thread::sleep(Duration::from_millis(250));
    page.press(Key::Right)?;
    std::thread::sleep(Duration::from_millis(400));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-map")?;

    // Arrow Up → PRs again, then drill into PR list.
    page.press(Key::Up)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Up)?;
    std::thread::sleep(Duration::from_millis(300));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-prs-again")?;

    // Enter → drill into PRs, Right → next PR.
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(400));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-pr-drill")?;
    page.press(Key::Right)?;
    std::thread::sleep(Duration::from_millis(500));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-next-pr")?;

    // Esc → exit drill, Down → Canvas, Enter → drill into canvas.
    page.press(Key::Esc)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(400));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-canvas-drill")?;
    page.press(Key::Esc)?;
    std::thread::sleep(Duration::from_millis(250));

    // --- Jobs tab ---
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(300));
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(600));
    capture_frame(&page, &frame_dir, &mut frames, "jobs")?;

    // --- Bugs tab ---
    page.press(Key::Char('b'))?;
    std::thread::sleep(Duration::from_millis(600));
    capture_frame(&page, &frame_dir, &mut frames, "bugs")?;

    page.kill()?;
    write_gif(&frames, std::path::Path::new(&output))?;

    Ok(())
}

/// Capture high-quality, font-rendered PNG screenshots of the README hero tabs
/// using tuiwright's TerminalRenderer (embedded JetBrains Mono TTF).
#[test]
#[ignore] // Run manually or in CI via `cargo test --test tui_recording`
fn tui_readme_screenshots() -> anyhow::Result<()> {
    let out_dir = std::env::var("JERYU_TUI_SCREENSHOTS_DIR")
        .unwrap_or_else(|_| "target/ci-screenshots".to_string());
    std::fs::create_dir_all(&out_dir)?;

    let bin = jeryu_bin();

    // Use 160x44 to match the existing README media dimensions.
    let cols: u16 = 160;
    let rows: u16 = 44;

    for (tab, output_name) in [
        ("workflow", "tui-workflow.png"),
        ("jobs", "tui-jobs.png"),
        ("bugs", "tui-bugs.png"),
    ] {
        let config = SpawnConfig::new(&bin)
            .args(["tui", "--demo", "--tab", tab])
            .size(cols, rows)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("JERYU_TUI_WORKFLOW_INSPECT_OPEN", "1")
            .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url());

        let page = Page::spawn(config)?;

        // Wait for synthetic demo data to render fully.
        std::thread::sleep(Duration::from_millis(1200));

        let path = std::path::PathBuf::from(&out_dir).join(output_name);
        page.screenshot(&path)?;
        page.kill()?;

        // Verify the output exists and is non-trivial.
        let meta = std::fs::metadata(&path)?;
        anyhow::ensure!(
            meta.len() > 10_000,
            "screenshot {} is suspiciously small ({} bytes)",
            path.display(),
            meta.len()
        );
    }

    Ok(())
}

fn capture_frame(
    page: &Page,
    frame_dir: &std::path::Path,
    frames: &mut Vec<std::path::PathBuf>,
    label: &str,
) -> anyhow::Result<()> {
    let path = frame_dir.join(format!("{:02}-{label}.png", frames.len()));
    page.screenshot(&path)?;
    frames.push(path);
    Ok(())
}

fn write_gif(frames: &[std::path::PathBuf], output: &std::path::Path) -> anyhow::Result<()> {
    anyhow::ensure!(!frames.is_empty(), "cannot write a GIF with no frames");
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let first = load_gif_frame(&frames[0])?;
    let (width, height) = first.dimensions();
    let mut file = std::fs::File::create(output)?;
    let mut encoder = gif::Encoder::new(&mut file, width as u16, height as u16, &[])?;
    encoder.set_repeat(gif::Repeat::Infinite)?;

    for frame_path in frames {
        let image = load_gif_frame(frame_path)?;
        anyhow::ensure!(
            image.dimensions() == (width, height),
            "GIF frame dimensions changed for {}",
            frame_path.display()
        );
        let mut frame = gif::Frame::from_rgb_speed(width as u16, height as u16, image.as_raw(), 10);
        frame.delay = 70;
        encoder.write_frame(&frame)?;
    }

    Ok(())
}

fn load_gif_frame(path: &std::path::Path) -> anyhow::Result<image::RgbImage> {
    Ok(image::open(path)?.to_rgb8())
}
