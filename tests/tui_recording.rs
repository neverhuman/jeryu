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
        .size(120, 34)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("JERYU_TUI_WORKFLOW_INSPECT_OPEN", "1")
        .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url());

    let page = Page::spawn(config)?;

    // Wait for initial synthetic activity to render.
    std::thread::sleep(Duration::from_millis(900));

    let mut frames = Vec::new();
    capture_frame(&page, &frame_dir, &mut frames, "workflow")?;

    // Workflow: drill into PRs and canvas so the recording shows activity and
    // the macro/micro focus affordances.
    std::thread::sleep(Duration::from_millis(500));
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(300));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-pr-drill")?;
    page.press(Key::Right)?;
    std::thread::sleep(Duration::from_millis(500));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-next-pr")?;
    page.press(Key::Esc)?;
    std::thread::sleep(Duration::from_millis(250));
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(250));
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(250));
    page.press(Key::Right)?;
    std::thread::sleep(Duration::from_millis(500));
    capture_frame(&page, &frame_dir, &mut frames, "workflow-canvas-drill")?;
    page.press(Key::Esc)?;

    // Mission tab.
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(700));
    capture_frame(&page, &frame_dir, &mut frames, "mission")?;

    // Release tab.
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(700));
    capture_frame(&page, &frame_dir, &mut frames, "release")?;

    // Jobs tab with fullscreen activity drill.
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(350));
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(250));
    page.press(Key::Enter)?;
    std::thread::sleep(Duration::from_millis(500));
    capture_frame(&page, &frame_dir, &mut frames, "jobs-log-drill")?;
    page.press(Key::Esc)?;
    std::thread::sleep(Duration::from_millis(250));

    // Agents, Tests, and Bugs show the rest of the synthetic control-plane
    // story without making the README animation too long.
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(550));
    capture_frame(&page, &frame_dir, &mut frames, "agents")?;
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(350));
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(450));
    capture_frame(&page, &frame_dir, &mut frames, "tests")?;
    page.press(Key::Char('b'))?;
    std::thread::sleep(Duration::from_millis(650));
    capture_frame(&page, &frame_dir, &mut frames, "bugs")?;

    page.kill()?;
    write_gif(&frames, std::path::Path::new(&output))?;

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
        let mut frame = gif::Frame::from_rgb_speed(width as u16, height as u16, image.as_raw(), 30);
        frame.delay = 70;
        encoder.write_frame(&frame)?;
    }

    Ok(())
}

fn load_gif_frame(path: &std::path::Path) -> anyhow::Result<image::RgbImage> {
    let image = image::open(path)?.to_rgb8();
    let max_width = 720;
    if image.width() <= max_width {
        return Ok(image);
    }
    let height = image.height() * max_width / image.width();
    Ok(image::imageops::resize(
        &image,
        max_width,
        height,
        image::imageops::FilterType::Triangle,
    ))
}
