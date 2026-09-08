//! Workcell model implementations kept separate from their public type definitions.

use super::*;

const WORKCELL_MAX_BRANCH_BUDGET: u32 = 5;

impl ArchiveEntryKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Hardlink => "hardlink",
            Self::CharacterDevice => "character-device",
            Self::BlockDevice => "block-device",
            Self::Fifo => "fifo",
            Self::Socket => "socket",
        }
    }
}

impl ArchiveEntry {
    pub fn new(path: impl Into<PathBuf>, kind: ArchiveEntryKind) -> Self {
        Self {
            path: path.into(),
            kind,
            link_target: None,
        }
    }
}

impl StartupSync {
    pub fn is_rebased(&self) -> bool {
        matches!(self, Self::Rebased { .. })
    }
}

impl BranchPolicy {
    pub fn new(agent_id: impl Into<String>, workcell_id: impl Into<String>, budget: u32) -> Self {
        Self {
            agent_id: agent_id.into(),
            workcell_id: workcell_id.into(),
            max_branches: budget.clamp(1, WORKCELL_MAX_BRANCH_BUDGET),
            open_branches: BTreeSet::new(),
            merge_allowed: false,
            delete_allowed: false,
        }
    }

    pub fn namespaced_branch(&self, branch: &str) -> String {
        format!(
            "agents/{}/workcells/{}/{}",
            self.agent_id, self.workcell_id, branch
        )
    }

    pub fn open_branch(&mut self, branch: impl Into<String>) -> WorkcellResult<String> {
        if self.open_branches.len() as u32 >= self.max_branches {
            return Err(WorkcellError::branch_budget_denied(format!(
                "agent {} hit workcell {} branch budget {}",
                self.agent_id, self.workcell_id, self.max_branches
            )));
        }
        let namespaced = self.namespaced_branch(&branch.into());
        self.open_branches.insert(namespaced.clone());
        Ok(namespaced)
    }

    pub fn allow_merge(&self) -> WorkcellResult<()> {
        if self.merge_allowed {
            Ok(())
        } else {
            Err(WorkcellError::merge_denied(format!(
                "merge control stays in review/queue for agent {} workcell {}",
                self.agent_id, self.workcell_id
            )))
        }
    }

    pub fn allow_delete(&self) -> WorkcellResult<()> {
        if self.delete_allowed {
            Ok(())
        } else {
            Err(WorkcellError::delete_denied(format!(
                "delete control stays out of agent {} workcell {}",
                self.agent_id, self.workcell_id
            )))
        }
    }
}

impl FrozenCiSnapshot {
    pub fn from_workcell(
        ci_run_id: impl Into<String>,
        failed_run_id: impl Into<String>,
        failed_receipt_id: impl Into<String>,
        workcell: &WorkcellLease,
        failure_log_digest: impl Into<String>,
        snapshot_age_ms: u64,
    ) -> Self {
        let allowed_paths = workcell
            .repo_roots
            .iter()
            .cloned()
            .chain(std::iter::once(workcell.workspace_root.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        Self {
            ci_run_id: ci_run_id.into(),
            failed_run_id: failed_run_id.into(),
            failed_receipt_id: failed_receipt_id.into(),
            workcell_id: workcell.workcell_id.clone(),
            runner_id: workcell.runner_id.clone(),
            runner_epoch: workcell.runner_epoch,
            workspace_root: workcell.workspace_root.clone(),
            repo_roots: workcell.repo_roots.clone(),
            allowed_paths,
            git_status_summary: workcell.git_status_summary.clone(),
            branch_policy: workcell.branch_policy.clone(),
            main_ref: workcell
                .startup_main_ref
                .clone()
                .unwrap_or_else(|| "main".to_string()),
            base_sha: workcell
                .startup_base_sha
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            head_sha: workcell
                .startup_head_sha
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            failure_log_digest: failure_log_digest.into(),
            snapshot_age_ms,
            heartbeat_healthy: workcell.heartbeat_healthy,
        }
    }
}

impl WorkcellLease {
    pub(super) fn ready(workcell_id: impl Into<String>) -> Self {
        let workcell_id = workcell_id.into();
        Self {
            workcell_id: workcell_id.clone(),
            state: WorkcellState::Ready,
            agent_id: String::new(),
            workspace_root: PathBuf::new(),
            repo_roots: Vec::new(),
            startup_head_sha: None,
            branch_policy: BranchPolicy::new("", workcell_id, 1),
            git_status_summary: String::new(),
            ci_snapshot_age_ms: None,
            runner_id: String::new(),
            runner_epoch: 0,
            heartbeat_healthy: false,
            startup_rebased: false,
            startup_main_ref: None,
            startup_base_sha: None,
            failed_run_id: None,
            failed_receipt_id: None,
            allowed_paths: Vec::new(),
            failure_log_digest: None,
            frozen_snapshot: None,
            blocked_reason: None,
        }
    }

    pub(super) fn apply_claim(&mut self, request: &WorkcellClaimRequest) {
        self.state = WorkcellState::Claimed;
        self.agent_id = request.agent_id.clone();
        self.workspace_root = request.workspace_root.clone();
        self.repo_roots = request.repo_roots.clone();
        self.branch_policy = BranchPolicy::new(
            request.agent_id.clone(),
            self.workcell_id.clone(),
            request.branch_budget,
        );
        self.git_status_summary = request.git_status_summary.clone();
        self.ci_snapshot_age_ms = request.ci_snapshot_age_ms;
        self.runner_id = request.runner_id.clone();
        self.runner_epoch = request.runner_epoch;
        self.heartbeat_healthy = true;
        self.startup_rebased = false;
        self.startup_main_ref = None;
        self.startup_base_sha = None;
        self.startup_head_sha = None;
        self.failed_run_id = None;
        self.failed_receipt_id = None;
        self.allowed_paths.clear();
        self.failure_log_digest = None;
        self.frozen_snapshot = None;
        self.blocked_reason = None;
    }

    pub(super) fn apply_startup(&mut self, startup: &StartupSync) {
        match startup {
            StartupSync::Rebased {
                main_ref,
                base_sha,
                head_sha,
            } => {
                self.startup_rebased = true;
                self.startup_main_ref = Some(main_ref.clone());
                self.startup_base_sha = Some(base_sha.clone());
                self.startup_head_sha = Some(head_sha.clone());
            }
            StartupSync::Failed {
                main_ref,
                base_sha,
                head_sha,
                reason,
            } => {
                self.startup_main_ref = Some(main_ref.clone());
                self.startup_base_sha = Some(base_sha.clone());
                self.startup_head_sha = Some(head_sha.clone());
                self.mark_blocked(format!("startup rebase failed: {reason}"));
            }
        }
    }

    pub(super) fn mark_blocked(&mut self, reason: impl Into<String>) {
        self.state = WorkcellState::Blocked;
        self.blocked_reason = Some(reason.into());
        self.heartbeat_healthy = false;
    }
}
