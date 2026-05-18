use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub async fn state_proof() -> Result<i32> {
    let container = match std::env::var("JERYU_REDLINE_PROOF_CONTAINER") {
        Ok(value) => value,
        Err(_) => "jeryu-redline-proof".to_string(),
    };
    let port = match std::env::var("JERYU_REDLINE_PROOF_PORT") {
        Ok(value) => value,
        Err(_) => "15439".to_string(),
    };
    let db = match std::env::var("JERYU_REDLINE_PROOF_DB") {
        Ok(value) => value,
        Err(_) => "jeryu_test".to_string(),
    };
    let user = match std::env::var("JERYU_REDLINE_PROOF_USER") {
        Ok(value) => value,
        Err(_) => "jeryu".to_string(),
    };
    let password = match std::env::var("JERYU_REDLINE_PROOF_PASSWORD") {
        Ok(value) => value,
        Err(_) => "jeryu_test".to_string(),
    };
    let image = match std::env::var("JERYU_REDLINE_PROOF_IMAGE") {
        Ok(value) => value,
        Err(_) => "redlinedb/redline:latest".to_string(),
    };
    let url = format!("redline://{user}:{password}@127.0.0.1:{port}/{db}");

    let _ = Command::new("docker")
        .args(["rm", "-f", &container])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    let mut run = Command::new("docker");
    run.args([
        "run",
        "--rm",
        "-d",
        "--name",
        &container,
        "-e",
        &format!("REDLINE_DB={db}"),
        "-e",
        &format!("REDLINE_USER={user}"),
        "-e",
        &format!("REDLINE_PASSWORD={password}"),
        "-p",
        &format!("127.0.0.1:{port}:5432"),
        &image,
    ]);
    let status = run
        .status()
        .await
        .context("starting redline proof container")?;
    if !status.success() {
        bail!("docker run failed");
    }

    let cleanup = container.clone();
    let keep = match std::env::var("JERYU_KEEP_REDLINE_PROOF") {
        Ok(value) => value == "1",
        Err(_) => false,
    };
    let result = async {
        for _ in 0..30 {
            let ready = Command::new("docker")
                .args([
                    "exec",
                    &container,
                    "redline-healthcheck",
                    "-U",
                    &user,
                    "-d",
                    &db,
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .context("probing redline readiness")?;
            if ready.success() {
                let mut test = Command::new("cargo");
                test.args([
                    "test",
                    "-p",
                    "jeryu",
                    "state::tests::redline_backend_smoke_test_when_configured",
                    "--",
                    "--nocapture",
                ]);
                test.env("JERYU_TEST_REDLINE_URL", &url);
                crate::exec::run_status_check(&mut test, "running redline proof test").await?;
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        bail!("redline proof container did not become ready");
    }
    .await;

    if !keep {
        let _ = Command::new("docker")
            .args(["rm", "-f", &cleanup])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    result?;
    Ok(0)
}

pub async fn capture_tui_screenshots(output_dir: Option<PathBuf>) -> Result<i32> {
    let root = repo_root()?;
    let output_dir = match output_dir {
        Some(path) => path,
        None => root.join("paper/assets"),
    };
    let debug_dir = root.join("target/tui-capture");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating {}", output_dir.display()))?;
    fs::create_dir_all(&debug_dir).with_context(|| format!("creating {}", debug_dir.display()))?;

    let cols = std::env::var("JERYU_TUI_CAPTURE_COLS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(160);
    let rows = std::env::var("JERYU_TUI_CAPTURE_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(48);
    let font_path = match std::env::var("JERYU_TUI_CAPTURE_FONT") {
        Ok(value) => PathBuf::from(value),
        Err(_) => PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    };
    let font_size = std::env::var("JERYU_TUI_CAPTURE_FONT_SIZE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(19.0);
    let cell_w = std::env::var("JERYU_TUI_CAPTURE_CELL_W")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);
    let cell_h = std::env::var("JERYU_TUI_CAPTURE_CELL_H")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(23);
    let bg = match std::env::var("JERYU_TUI_CAPTURE_BG") {
        Ok(value) => value,
        Err(_) => "#17212b".to_string(),
    };
    let fg = match std::env::var("JERYU_TUI_CAPTURE_FG") {
        Ok(value) => value,
        Err(_) => "#f4fbff".to_string(),
    };
    let brighten = std::env::var("JERYU_TUI_CAPTURE_BRIGHTEN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1.35_f32);
    let max_wait_ms = std::env::var("JERYU_TUI_CAPTURE_MAX_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8000_u64);
    let min_wait_ms = std::env::var("JERYU_TUI_CAPTURE_MIN_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1200_u64);
    let quiet_ms = std::env::var("JERYU_TUI_CAPTURE_QUIET_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300_u64);

    let mut build = Command::new("cargo");
    build
        .args(["build", "--release", "-p", "jeryu", "-p", "tui-capture"])
        .current_dir(&root);
    crate::exec::run_status_check(&mut build, "building tui-capture assets").await?;

    let shots = [
        ("mission", output_dir.join("jeryu-tui-mission.png")),
        ("jobs", output_dir.join("jeryu-tui-jobs-flow.png")),
        ("agents", output_dir.join("jeryu-tui-agents.png")),
        ("tests", output_dir.join("jeryu-tui-tests-vti.png")),
        ("evidence", output_dir.join("jeryu-tui-evidence.png")),
        ("release", output_dir.join("jeryu-tui-release.png")),
    ];

    for (tab, output) in shots {
        let ready_file = tempfile::NamedTempFile::new().context("creating ready file")?;
        let mut cmd = Command::new(root.join("target/release/tui-capture"));
        cmd.arg("--cols").arg(cols.to_string());
        cmd.arg("--rows").arg(rows.to_string());
        cmd.arg("--out").arg(&output);
        cmd.arg("--font").arg(&font_path);
        cmd.arg("--font-size").arg(font_size.to_string());
        cmd.arg("--cell-w").arg(cell_w.to_string());
        cmd.arg("--cell-h").arg(cell_h.to_string());
        cmd.arg("--bg").arg(&bg);
        cmd.arg("--fg").arg(&fg);
        cmd.arg("--brighten").arg(brighten.to_string());
        cmd.arg("--min-wait-ms").arg(min_wait_ms.to_string());
        cmd.arg("--max-wait-ms").arg(max_wait_ms.to_string());
        cmd.arg("--quiet-ms").arg(quiet_ms.to_string());
        cmd.arg("--ready-file").arg(ready_file.path());
        cmd.arg("--dump-text")
            .arg(debug_dir.join(format!("{tab}.txt")));
        cmd.arg("--");
        cmd.arg(root.join("target/release/jeryu"));
        cmd.arg("tui");
        cmd.arg("--screenshot");
        cmd.arg("--tab").arg(tab);
        cmd.arg("--screenshot-hold-ms").arg("10000");
        crate::exec::run_status_check(&mut cmd, &format!("tui capture failed for {tab}")).await?;
        if !ready_file.path().exists() {
            bail!("TUI did not signal readiness for {tab}");
        }
    }
    Ok(0)
}

fn repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("resolving current directory")?;
    loop {
        if dir.join("Cargo.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("unable to locate repo root");
        }
    }
}
