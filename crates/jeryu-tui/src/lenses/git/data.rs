//! Owner: Interactive TUI subsystem - Git lens data selector
//! Proof: `cargo test -p jeryu-tui --lib lenses::git::data`
//! Invariants: Pure projection from the recent git-command event ledger to
//!             `GitLensInput`. No I/O. Rows are owned clones of the redacted
//!             fields only — never a non-redacted argv.

/// Source record for one git-command event.
#[derive(Debug, Clone, Default)]
pub struct GitCommandEventRecord {
    pub id: i64,
    pub request_id: String,
    pub actor: String,
    pub cwd: String,
    pub repo_root: Option<String>,
    pub argv_redacted: String,
    pub argv_hash: String,
    pub command_class: String,
    pub risk: String,
    pub mode: String,
    pub before_head: Option<String>,
    pub before_branch: Option<String>,
    pub before_dirty: Option<String>,
    pub after_head: Option<String>,
    pub after_branch: Option<String>,
    pub after_dirty: Option<String>,
    pub exit_code: i32,
    pub sidecar_status: String,
    pub mirror_status: String,
    pub created_at: String,
    pub payload: String,
}

/// One projected row of the git command / sync ledger.
#[derive(Debug, Clone, Default)]
pub struct GitEventRow {
    pub created_at: String,
    pub command_class: String,
    pub exit_code: i32,
    pub mirror_status: String,
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

    pub fn status(&self) -> &'static str {
        if self.exit_code == 0 {
            "success"
        } else {
            "failed"
        }
    }

    pub fn failed(&self) -> bool {
        self.exit_code != 0
    }
}

/// Owned, render-ready projection of the recent git command ledger.
#[derive(Debug, Clone, Default)]
pub struct GitLensInput {
    pub rows: Vec<GitEventRow>,
    pub selected: usize,
}

impl GitLensInput {
    pub fn from_state(events: &[GitCommandEventRecord], selected: usize) -> Self {
        let rows = events.iter().map(GitEventRow::from_record).collect();
        Self { rows, selected }
    }

    /// Project a minimal summary from the read model (no git events available
    /// through the contract yet).
    pub fn from_read_model(_model: &jeryu_readmodel::TuiReadModel) -> Self {
        Self {
            rows: Vec::new(),
            selected: 0,
        }
    }

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
