//! Session-launch seam for the Agents lens "New Session" affordance.
//!
//! Pressing `n` on the Agents tab creates a fresh, isolated agent session on
//! the active repository. The lens drives that intent through [`SessionLauncher`]
//! so the render/input layer stays transport-agnostic — exactly like the
//! [`TtySource`](crate::runtime::tty::TtySource) /
//! [`ControlSink`](crate::runtime::tty::ControlSink) pair drives the live TTY.
//!
//! The production launcher plugs in behind this trait later (POSTing
//! `/api/v1/repos/{id}/sessions` and decoding the
//! [`SessionHandle`](SessionHandle) from the response), while the tests drive the
//! same surface with the scripted [`RecordingSessionLauncher`] fake — no HTTP, no
//! control plane.

/// The handle the control plane returns when a new session is created
/// (`POST /api/v1/repos/{id}/sessions`). The lens attaches a live terminal on
/// `run_id` and surfaces `branch` in the launch banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHandle {
    /// The created agent run id (scope `agent_run.{run_id}`).
    pub run_id: String,
    /// The isolated working branch the session operates on.
    pub branch: String,
}

impl SessionHandle {
    /// Build a handle from its run id and working branch.
    pub fn new(run_id: impl Into<String>, branch: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            branch: branch.into(),
        }
    }
}

/// A creator of isolated agent sessions for a repository. The Agents lens calls
/// this when the operator presses `n`; the returned [`SessionHandle`] names the
/// new run the lens then attaches its live terminal to.
pub trait SessionLauncher {
    /// Create a new isolated session on `repo_id` and return its handle.
    fn create_session(&mut self, repo_id: &str) -> SessionHandle;
}

#[cfg(test)]
pub use fakes::RecordingSessionLauncher;

#[cfg(test)]
mod fakes {
    use super::{SessionHandle, SessionLauncher};

    /// A [`SessionLauncher`] that records every repository it was asked to
    /// launch a session on and hands back a deterministic [`SessionHandle`], so
    /// the lens interaction can be proven without a backend.
    #[derive(Debug, Default)]
    pub struct RecordingSessionLauncher {
        /// Every `repo_id` passed to [`create_session`], in call order.
        pub requested: Vec<String>,
        next_seq: u64,
    }

    impl RecordingSessionLauncher {
        pub fn new() -> Self {
            Self::default()
        }

        /// The most recent repository a session launch was requested for.
        pub fn last(&self) -> Option<&String> {
            self.requested.last()
        }
    }

    impl SessionLauncher for RecordingSessionLauncher {
        fn create_session(&mut self, repo_id: &str) -> SessionHandle {
            self.next_seq += 1;
            self.requested.push(repo_id.to_string());
            SessionHandle::new(
                format!("agent_run.session-{}", self.next_seq),
                format!("agent/session-{}", self.next_seq),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_launcher_captures_repo_and_mints_handles() {
        let mut launcher = RecordingSessionLauncher::new();
        let first = launcher.create_session("core/web");
        let second = launcher.create_session("core/api");

        assert_eq!(launcher.requested, vec!["core/web", "core/api"]);
        assert_eq!(launcher.last(), Some(&"core/api".to_string()));
        assert_eq!(first.run_id, "agent_run.session-1");
        assert_eq!(first.branch, "agent/session-1");
        assert_ne!(first, second);
    }
}
