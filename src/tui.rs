//! Owner: Interactive TUI subsystem (module root)
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: TUI entry points preserve terminal cleanup and keep operational actions policy-gated.

pub mod action_registry;
pub mod activity;
pub mod app;
pub mod bugs;
pub mod flow;
pub mod focus;
pub mod jankurai;
pub mod live;
pub mod runner;
pub mod runtime;
pub mod theme;
pub mod ui;
pub mod widgets;
pub mod workflow;

pub use runner::{capture_tui_png, run_tui, run_tui_once, run_tui_screenshot};

#[cfg(test)]
pub(crate) mod test_support {
    use anyhow::Result;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    pub(crate) fn docker_ctl() -> Result<crate::docker::DockerCtl> {
        static FAKE_DOCKER_SOCKET: OnceLock<PathBuf> = OnceLock::new();
        let socket_path = FAKE_DOCKER_SOCKET.get_or_init(|| {
            let path =
                std::env::temp_dir().join(format!("jeryu-tui-docker-{}.sock", std::process::id()));
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&path);
            // SAFETY: test-only process-local override. No production code reads this helper.
            unsafe {
                std::env::set_var("DOCKER_HOST", format!("unix://{}", path.display()));
            }
            path
        });

        if !socket_path.exists() {
            std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(socket_path)?;
        }

        crate::docker::DockerCtl::connect()
    }
}
