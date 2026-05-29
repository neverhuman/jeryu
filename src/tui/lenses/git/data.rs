//! Owner: Interactive TUI subsystem - Git lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::git::data`
//! Invariants: Pure projection from the recent git-command event ledger to
//!             `GitLensInput`. No I/O. Rows are owned clones of the redacted
//!             fields only — never a non-redacted argv. Replaces the legacy
//!             git-sync panel (`draw_git_tab`) with a lens projecting from
//!             app state.

use crate::state::GitCommandEventRecord;

/// One projected row of the git command / sync ledger.
///
/// Every field is an owned clone of an already-redacted source field. The raw
/// `argv` is never carried — only `command` (from `argv_redacted`).
#[derive(Debug, Clone, Default)]
pub struct GitEventRow {
    /// Event timestamp (already a string in the source record).
    pub created_at: String,
    /// Command class bucket (e.g. `push`, `fetch`, `commit`).
    pub command_class: String,
    /// Process exit code; `0` means success, anything else failed.
    pub exit_code: i32,
    /// External-mirror sync status word (e.g. `synced`, `pending`, `n/a`).
    pub mirror_status: String,
    /// Redacted argv — safe to render verbatim.
    pub argv_redacted: String,
}

impl GitEventRow {
    fn from_record(record: &GitCommandEventRecord) -> Self {
        Self {
            created_at: record.created_at.clone(),
            command_class: record.command_class.clone(),
            exit_code: record.exit_code,
            mirror_status: record.mirror_status.clone(),
            argv_redacted: record.argv_redacted.clone(),
        }
    }

    /// Low-noise status word derived from the exit code.
    pub fn status(&self) -> &'static str {
        if self.exit_code == 0 {
            "success"
        } else {
            "failed"
        }
    }

    /// Whether this operation failed (non-zero exit).
    pub fn failed(&self) -> bool {
        self.exit_code != 0
    }
}

/// Owned, render-ready projection of the recent git command ledger.
#[derive(Debug, Clone, Default)]
pub struct GitLensInput {
    /// Recent git events, newest-first as supplied by the source ledger.
    pub rows: Vec<GitEventRow>,
    /// Index of the highlighted row (clamped to a valid row at render time).
    pub selected: usize,
}

impl GitLensInput {
    /// Project from the recent git-command event ledger. Rows are owned clones
    /// of the redacted fields; `selected` is preserved verbatim (the view
    /// clamps it for highlighting so an out-of-range cursor never panics).
    pub fn from_state(events: &[GitCommandEventRecord], selected: usize) -> Self {
        let rows = events.iter().map(GitEventRow::from_record).collect();
        Self { rows, selected }
    }

    /// Number of git operations recorded that failed (non-zero exit code).
    pub fn failed_count(&self) -> usize {
        self.rows.iter().filter(|r| r.failed()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(class: &str, exit: i32, mirror: &str, argv: &str) -> GitCommandEventRecord {
        GitCommandEventRecord {
            id: 1,
            request_id: "req".into(),
            actor: "actor".into(),
            cwd: "/repo".into(),
            repo_root: Some("/repo".into()),
            argv_redacted: argv.into(),
            argv_hash: "hash".into(),
            command_class: class.into(),
            risk: "low".into(),
            mode: "exec".into(),
            before_head: None,
            before_branch: None,
            before_dirty: None,
            after_head: None,
            after_branch: None,
            after_dirty: None,
            exit_code: exit,
            sidecar_status: "ok".into(),
            mirror_status: mirror.into(),
            created_at: "2026-05-29T12:00:00Z".into(),
            payload: "{}".into(),
        }
    }

    #[test]
    fn empty_events_produce_empty_rows() {
        let input = GitLensInput::from_state(&[], 0);
        assert!(input.rows.is_empty());
        assert_eq!(input.selected, 0);
        assert_eq!(input.failed_count(), 0);
    }

    #[test]
    fn selected_is_preserved() {
        let events = vec![
            record("push", 0, "synced", "git push origin main"),
            record("fetch", 1, "pending", "git fetch --all"),
        ];
        let input = GitLensInput::from_state(&events, 1);
        assert_eq!(input.selected, 1);
        assert_eq!(input.rows.len(), 2);
        // Even an out-of-range selection is preserved as-given (view clamps).
        let over = GitLensInput::from_state(&events, 99);
        assert_eq!(over.selected, 99);
    }

    #[test]
    fn rows_clone_redacted_fields_and_classify_status() {
        let events = vec![
            record("push", 0, "synced", "git push origin main"),
            record("fetch", 128, "n/a", "git fetch --all"),
        ];
        let input = GitLensInput::from_state(&events, 0);
        assert_eq!(input.rows[0].command_class, "push");
        assert_eq!(input.rows[0].mirror_status, "synced");
        assert_eq!(input.rows[0].argv_redacted, "git push origin main");
        assert_eq!(input.rows[0].status(), "success");
        assert!(!input.rows[0].failed());
        assert_eq!(input.rows[1].status(), "failed");
        assert!(input.rows[1].failed());
        assert_eq!(input.failed_count(), 1);
    }
}
