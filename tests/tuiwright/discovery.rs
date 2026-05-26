//! Owner: Tuiwright test suite - discovery
//! Proof: `cargo nextest run --test tuiwright -- discovery::`
//! Invariants: every assertion preserved from the pre-split tests/tui_tuiwright.rs

use std::time::Duration;
use tuiwright::{Page, SpawnConfig};

use crate::helpers::*;

/// When `JERYU_WORKSPACE_ROOT` is set to a directory that contains
/// `.jeryu/repos.toml`, the fleet bar should render those repo aliases even
/// if the TUI was launched from an unrelated working directory.
///
/// This test exercises the Strategy-1 (env-override) path of
/// `resolve_workspace_root()` end-to-end through the TUI render loop.
/// It does NOT use `--demo`; instead it provides a synthetic workspace root
/// with two custom aliases ("myalpha", "mybeta") that are impossible to
/// confuse with the demo fixture or any production repo.
///
/// **CI bootstrap notes** — non-demo mode calls `load_client()` which needs a
/// GitLab token in the auth chain.  We point `GITLAB_URL` at port 1 (always
/// ECONNREFUSED) and set `GITLAB_TOKEN` to a dummy value so the auth resolver
/// returns immediately (non-local URL path, no validation call).  Every
/// subsequent GitLab API call also gets ECONNREFUSED with no 30 s timeout.
#[test]
fn fleet_bar_discovers_repos_via_workspace_root_env() -> anyhow::Result<()> {
    use tempfile::TempDir;

    let _guard = tuiwright_lock();

    // Build a synthetic .jeryu/repos.toml with unique aliases.
    let tmp = TempDir::new()?;
    let registry_dir = tmp.path().join(".jeryu");
    std::fs::create_dir_all(&registry_dir)?;
    std::fs::write(
        registry_dir.join("repos.toml"),
        r#"schema_version = "1"
workspace = "synthetic-test"

[[repo]]
alias = "myalpha"
slug = "testorg/myalpha"
provider = "github"
remote = "https://github.com/testorg/myalpha.git"
local_root = "/tmp/myalpha-does-not-exist"
default_branch = "main"
visibility = "private"
health_profile = "rust-workspace"

[[repo]]
alias = "mybeta"
slug = "testorg/mybeta"
provider = "github"
remote = "https://github.com/testorg/mybeta.git"
local_root = "/tmp/mybeta-does-not-exist"
default_branch = "main"
visibility = "private"
health_profile = "rust-workspace"
"#,
    )?;

    // Launch the TUI without --demo.  The dispatch layer calls load_client()
    // which reads GITLAB_TOKEN from the environment.  We provide:
    //   GITLAB_URL=http://127.0.0.1:1  — non-local URL (port 1 ≠ GITLAB_HTTP_PORT)
    //   GITLAB_TOKEN=dummy             — satisfies the auth chain immediately
    // Because the URL is non-local, resolve_or_repair returns the dummy token
    // without making any network call.  All subsequent GitLab API calls get
    // ECONNREFUSED at once (port 1), so hydrate_smoke_state completes quickly.
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--tab")
            .arg("workflow")
            .size(220, 44)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("NO_COLOR", "")
            .env("GITLAB_URL", "http://127.0.0.1:1")
            .env("GITLAB_TOKEN", "dummy-tuiwright-token")
            .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url())
            .env("JERYU_WORKSPACE_ROOT", tmp.path().to_str().unwrap())
            .timeout(Duration::from_secs(15)),
    )?;

    // Give the TUI time to run hydrate_smoke_state + initial render.
    // Use 5 s to absorb any CI timing slack.
    std::thread::sleep(Duration::from_millis(5000));

    let text = screen_text(&page);

    // Both synthetic aliases must appear in the fleet bar.
    assert!(
        text.contains("myalpha"),
        "fleet bar should show 'myalpha' from JERYU_WORKSPACE_ROOT repos.toml; screen:\n{text}"
    );
    assert!(
        text.contains("mybeta"),
        "fleet bar should show 'mybeta' from JERYU_WORKSPACE_ROOT repos.toml; screen:\n{text}"
    );

    // tmp stays alive until here — repos.toml is present for the full test.
    drop(tmp);
    Ok(())
}

/// Fleet bar — Strategy 4 (serve-daemon cwd): when a `jeryu serve` process is
/// running from a directory that has `.jeryu/repos.toml`, and the TUI is
/// launched from an unrelated directory *without* `JERYU_WORKSPACE_ROOT`, the
/// fleet bar should still render repo aliases from the daemon's workspace.
///
/// The test is conditional: if no `jeryu serve` daemon is running in the
/// current environment, it is skipped (returns Ok immediately).  This makes it
/// safe in clean CI runners while still catching regressions locally and on
/// machines with the daemon running.
#[test]
fn fleet_bar_discovers_repos_via_serve_daemon_strategy4() -> anyhow::Result<()> {
    // Inline /proc scan to detect a running jeryu serve daemon.  This mirrors
    // the logic in `repo_fleet::serve_daemon_workspace_root` without needing
    // to export that private function.
    const REGISTRY: &str = ".jeryu/repos.toml";
    let daemon_workspace: Option<std::path::PathBuf> = (|| {
        let entries = std::fs::read_dir("/proc").ok()?;
        for entry in entries.flatten() {
            let pid_dir = entry.path();
            if !pid_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
            {
                continue;
            }
            let Ok(cmdline) = std::fs::read(pid_dir.join("cmdline")) else {
                continue;
            };
            let args: Vec<&[u8]> = cmdline.split(|&b| b == 0).collect();
            let is_jeryu = args
                .first()
                .and_then(|a| std::str::from_utf8(a).ok())
                .map(|a| a.ends_with("jeryu") || a.ends_with("/jeryu"))
                .unwrap_or(false);
            let has_serve = args.iter().skip(1).any(|a| *a == b"serve");
            if !(is_jeryu && has_serve) {
                continue;
            }
            if let Ok(cwd) = std::fs::read_link(pid_dir.join("cwd"))
                && cwd.join(REGISTRY).exists()
            {
                return Some(cwd);
            }
        }
        None
    })();

    let Some(daemon_root) = daemon_workspace else {
        eprintln!(
            "fleet_bar_discovers_repos_via_serve_daemon_strategy4: SKIP — no jeryu serve daemon found"
        );
        return Ok(());
    };

    // Load the alias list from the daemon's repos.toml so we know what to
    // expect on screen (avoids hard-coding production aliases here).
    let registry_raw = std::fs::read_to_string(daemon_root.join(REGISTRY))?;
    let aliases: Vec<String> = registry_raw
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("alias = \"")
                .and_then(|s| s.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    assert!(
        !aliases.is_empty(),
        "daemon repos.toml must define at least one repo alias"
    );
    let first_alias = aliases[0].clone();

    let _guard = tuiwright_lock();

    // Launch TUI from /tmp (unrelated to the workspace) and without
    // JERYU_WORKSPACE_ROOT — so Strategy 4 must kick in.
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--tab")
            .arg("workflow")
            .size(220, 44)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("NO_COLOR", "")
            .env("GITLAB_URL", "http://127.0.0.1:1")
            .env("GITLAB_TOKEN", "dummy-tuiwright-token")
            .env("JERYU_DATABASE_URL", jeryu::db::config::sqlite_memory_url())
            // Deliberately omit JERYU_WORKSPACE_ROOT — Strategy 4 must find it
            .timeout(Duration::from_secs(20)),
    )?;

    // Give the background sync time to call resolve_workspace_root and render.
    std::thread::sleep(Duration::from_millis(6000));

    let text = screen_text(&page);

    assert!(
        text.contains(&first_alias),
        "fleet bar should show '{first_alias}' via Strategy 4 (serve daemon cwd); screen:\n{text}"
    );

    Ok(())
}
