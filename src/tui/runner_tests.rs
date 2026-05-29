use std::path::PathBuf;

use crate::{docker::DockerCtl, gitlab_client::GitlabClient};

use super::{capture_tui_png, run_tui_once};

fn docker() -> DockerCtl {
    DockerCtl::disconnected()
}

fn gitlab() -> GitlabClient {
    GitlabClient::new("http://127.0.0.1:9", None)
}

#[tokio::test]
async fn run_once_smoke_renders_demo_without_store() -> anyhow::Result<()> {
    run_tui_once(None, docker(), gitlab(), "jobs").await
}

#[tokio::test]
async fn capture_png_smoke_honors_tab_and_size_flags() -> anyhow::Result<()> {
    let output = temp_capture_path("mission");
    let _ = std::fs::remove_file(&output);

    capture_tui_png(None, docker(), gitlab(), "mission", &output, 40, 12).await?;

    let image = image::open(&output)?;
    assert_eq!(image.width(), 40 * 8);
    assert_eq!(image.height(), 12 * 12);
    std::fs::remove_file(&output)?;
    Ok(())
}

fn temp_capture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("jeryu-tui-{name}-{}.png", std::process::id()))
}
