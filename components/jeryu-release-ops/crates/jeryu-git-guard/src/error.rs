//! Typed refusal errors and actionable repair guidance.

/// The verdict for a single `git …` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitDecision {
    /// The command is branch-local and safe; the wrapper may exec real git.
    Allow,
    /// The command is refused; the wrapper prints the error and exits non-zero.
    Deny(GitGuardError),
}

impl GitDecision {
    /// Whether this verdict permits running real git.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, GitDecision::Allow)
    }
}

/// Structured repair guidance for a refused git command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitGuardError {
    /// What the agent was apparently trying to do.
    pub purpose: &'static str,
    /// Stable machine-readable reason code (e.g. `git_network_denied`).
    pub reason: &'static str,
    /// Concrete things the agent can do instead.
    pub common_fixes: &'static [&'static str],
    /// Where to read more.
    pub docs_url: &'static str,
    /// One-line steer.
    pub repair_hint: &'static str,
    /// The exact refused command, for the log line.
    command: String,
}

const NETWORK_FIXES: &[&str] = &[
    "commit locally on your branch; jeryu publishes it on export/review",
    "you cannot push/fetch/clone — the cell has no network and jeryu mediates git",
];
const WORKTREE_FIXES: &[&str] = &[
    "you get exactly one branch in one runner",
    "need a second line of work? ask jeryu to start another runner — do not add worktrees",
];
const INTEGRATION_FIXES: &[&str] = &[
    "do not merge/rebase/cherry-pick — jeryu rebases your branch on main and integrates",
    "just commit your work; jeryu handles integration and conflict resolution",
];
const BRANCH_FIXES: &[&str] = &[
    "you are pinned to your assigned branch (JERYU_BRANCH)",
    "do not create/delete/rename/switch branches — request another runner for parallel work",
];
const CONFIG_WRITE_FIXES: &[&str] = &[
    "config is read-only inside the cell (identity/options are preset by jeryu)",
    "aliases and credential/remote config cannot be written from inside the cell",
];
const GLOBAL_FLAG_FIXES: &[&str] = &[
    "drop the -c/--config-env/--git-dir/--work-tree/--exec-path option",
    "run plain git subcommands inside your workspace; jeryu controls the repo layout",
];
const SUBCOMMAND_FIXES: &[&str] = &[
    "only branch-local commands are allowed (status/diff/log/show/add/commit/revert/reset/restore/stash)",
    "if you reached here via a git alias, it is refused: aliases can run arbitrary commands",
];
const COMMIT_VERIFY_FIXES: &[&str] = &[
    "drop --no-verify/-n: the jankurai audit gate must run on every commit",
    "if the gate blocked you, fix the introduced caps/findings and commit again",
];

impl GitGuardError {
    fn new(
        purpose: &'static str,
        reason: &'static str,
        common_fixes: &'static [&'static str],
        repair_hint: &'static str,
        command: String,
    ) -> Self {
        Self {
            purpose,
            reason,
            common_fixes,
            docs_url: "docs/errors.md#git-guard",
            repair_hint,
            command,
        }
    }

    /// The refused command string (`git push origin HEAD`).
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Render a human + agent readable refusal block.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "jeryu-git: refused `{}` — {}\n  why: {}\n  fixes:\n",
            self.command, self.reason, self.purpose
        );
        for fix in self.common_fixes {
            out.push_str("    - ");
            out.push_str(fix);
            out.push('\n');
        }
        out.push_str("  hint: ");
        out.push_str(self.repair_hint);
        out.push_str("\n  docs: ");
        out.push_str(self.docs_url);
        out
    }

    pub(crate) fn network(command: String) -> Self {
        Self::new(
            "reach the network or another repo via git",
            "git_network_denied",
            NETWORK_FIXES,
            "commit locally; jeryu publishes and integrates your branch",
            command,
        )
    }

    pub(crate) fn worktree(command: String) -> Self {
        Self::new(
            "create an extra worktree / submodule checkout",
            "git_worktree_denied",
            WORKTREE_FIXES,
            "more worktrees → fire up more runners (one branch per runner)",
            command,
        )
    }

    pub(crate) fn integration(command: String) -> Self {
        Self::new(
            "rewrite history or integrate other refs",
            "git_integration_denied",
            INTEGRATION_FIXES,
            "just commit; jeryu rebases on main and integrates",
            command,
        )
    }

    pub(crate) fn branch(command: String) -> Self {
        Self::new(
            "create, delete, rename, or switch branches",
            "git_branch_denied",
            BRANCH_FIXES,
            "you are confined to one branch; request another runner for more",
            command,
        )
    }

    pub(crate) fn config_write(command: String) -> Self {
        Self::new(
            "write git config (remote/credential/alias/identity)",
            "git_config_write_denied",
            CONFIG_WRITE_FIXES,
            "git config is read-only in the cell",
            command,
        )
    }

    pub(crate) fn global_flag(command: String) -> Self {
        Self::new(
            "use a config-injecting or repo-redirecting global flag",
            "git_global_flag_denied",
            GLOBAL_FLAG_FIXES,
            "remove the -c/--git-dir/--work-tree/--exec-path/--config-env option",
            command,
        )
    }

    pub(crate) fn subcommand(command: String) -> Self {
        Self::new(
            "run a git subcommand outside the branch-local allowlist",
            "git_subcommand_denied",
            SUBCOMMAND_FIXES,
            "use a branch-local command, or ask jeryu for the operation you need",
            command,
        )
    }

    pub(crate) fn commit_unverified(command: String) -> Self {
        Self::new(
            "skip the pre-commit jankurai audit gate",
            "git_commit_unverified_denied",
            COMMIT_VERIFY_FIXES,
            "remove --no-verify; resolve the audit findings instead",
            command,
        )
    }
}
