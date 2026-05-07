use std::time::Duration;
use tuiwright::{GifOptions, Key, Page, SpawnConfig};

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
    
    let bin = jeryu_bin();
    let config = SpawnConfig::new(&bin)
        .args(["tui", "--demo"])
        .size(140, 44)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor");

    let page = Page::spawn(config)?;

    // Wait for initial render
    std::thread::sleep(Duration::from_millis(1500));

    // Start recording GIF
    page.start_recording()?;

    // Wait 3 seconds on the default Workflow tab
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Mission tab (1)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Release tab (2)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Jobs tab (3)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Agents tab (4)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Tests tab (5)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(1500));

    // Scroll through the tests
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(1500));
    page.press(Key::Down)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Pools tab (6)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Cache tab (7)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Evidence tab (8)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Go to Secrets tab (9)
    page.press(Key::Tab)?;
    std::thread::sleep(Duration::from_millis(3000));

    // Save the GIF
    let gif_options = GifOptions {
        loop_forever: true,
        max_fps: 12,
        ..Default::default()
    };
    
    page.stop_recording_gif("target/ci-screenshots/tui-demo.gif", gif_options)?;

    Ok(())
}
