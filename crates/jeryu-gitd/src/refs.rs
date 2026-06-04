//! Git ref operations.

use crate::command::run_capture;
use crate::error::{GitdError, Result};
use crate::object_fsck::ObjectFsck;
use crate::protection::{RefChange, RefOperation};
use crate::repo::{RepoManager, Repository};

/// A Git ref and object id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRef {
    /// Ref name such as `refs/heads/main`.
    pub name: String,
    /// Object id as hex.
    pub oid: String,
}

/// Result of a real pull-request merge that advanced the base ref.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeOutcome {
    /// Object id the base ref now points at: the head oid for a fast-forward,
    /// or the brand-new two-parent merge commit oid otherwise.
    pub merge_oid: String,
    /// Whether the merge was a pure fast-forward (no new commit object created).
    pub fast_forward: bool,
}

/// Ref service enforcing protected-ref policy before mutation.
#[derive(Clone, Debug)]
pub struct RefService {
    manager: RepoManager,
}

impl RefService {
    /// Create a ref service.
    #[must_use]
    pub fn new(manager: RepoManager) -> Self {
        Self { manager }
    }

    /// List refs in a bare repository.
    pub fn list_refs(&self, repo: &Repository) -> Result<Vec<GitRef>> {
        let out = run_capture(
            &self.manager.config().git_bin,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
            Some(&repo.path),
        )?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut refs = Vec::new();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            let Some(oid) = parts.next() else { continue };
            refs.push(GitRef {
                name: name.to_string(),
                oid: oid.to_string(),
            });
        }
        Ok(refs)
    }

    /// Update a ref with policy checks.
    pub fn update_ref(
        &self,
        repo: &Repository,
        actor: &str,
        name: &str,
        new_oid: &str,
        old_oid: Option<&str>,
    ) -> Result<()> {
        validate_ref_name(name)?;
        let operation = if is_zero_oid(new_oid) {
            RefOperation::Delete
        } else if old_oid.is_some() {
            RefOperation::Update
        } else {
            RefOperation::Create
        };
        let force = force_update(&self.manager, repo, name, new_oid, old_oid, operation)?;
        let change = RefChange {
            actor: actor.to_string(),
            ref_name: name.to_string(),
            old_oid: old_oid.unwrap_or(ZERO_OID).to_string(),
            new_oid: new_oid.to_string(),
            operation,
            force,
        };
        for rule in &self.manager.config().protected_refs {
            rule.evaluate(&change)?;
        }
        let mut args = vec!["update-ref", name, new_oid];
        if let Some(prior_oid) = old_oid {
            args.push(prior_oid);
        }
        run_capture(&self.manager.config().git_bin, &args, Some(&repo.path))?;
        Ok(())
    }

    /// Perform a real, server-side pull-request merge that advances `base_ref`
    /// from `base_oid` to either the head commit (fast-forward) or a new
    /// two-parent merge commit, returning the resulting oid.
    ///
    /// The advance goes through [`RefService::update_ref`], so the protected-ref
    /// policy and the old-oid compare-and-swap both still apply. Because
    /// `base_oid` is always an ancestor of the produced oid (in both the
    /// fast-forward and merge-commit cases), this is a sanctioned non-force
    /// advance: `deny_force` on `refs/heads/main` is satisfied without any
    /// bypass actor.
    ///
    /// `require_fast_forward` enforces linear history: when set, a divergent
    /// head (one that cannot fast-forward the base) is refused before any commit
    /// is created or any ref is moved.
    #[allow(clippy::too_many_arguments)]
    pub fn merge_pull(
        &self,
        repo: &Repository,
        actor: &str,
        base_ref: &str,
        base_oid: &str,
        head_oid: &str,
        message: &str,
        require_fast_forward: bool,
    ) -> Result<MergeOutcome> {
        validate_ref_name(base_ref)?;
        if is_zero_oid(base_oid) || is_zero_oid(head_oid) {
            return Err(GitdError::InvalidInput(
                "merge requires non-zero base and head oids".to_string(),
            ));
        }

        let git_bin = self.manager.config().git_bin.clone();
        // Both oids must resolve to real commits in the bare repo. This rejects
        // synthetic shas (e.g. a stale `merge-…` placeholder) before they could
        // smuggle into a real ref move.
        verify_commit(&git_bin, repo, base_oid)?;
        verify_commit(&git_bin, repo, head_oid)?;

        let ff = ObjectFsck::new(git_bin.clone()).is_ancestor(repo, base_oid, head_oid)?;

        if require_fast_forward && !ff {
            return Err(GitdError::NonFastForwardRequired);
        }

        let (merge_oid, fast_forward) = if ff {
            // Advancing the base to the head commit IS the merge; no new object.
            (head_oid.to_string(), true)
        } else {
            let tree_oid = merge_tree(&git_bin, repo, base_oid, head_oid)?;
            let merge_commit = commit_tree(&git_bin, repo, &tree_oid, base_oid, head_oid, message)?;
            (merge_commit, false)
        };

        // Sanctioned non-force advance via the CAS-guarded, protection-checked
        // path. Passing Some(base_oid) makes a concurrent advance fail loudly.
        self.update_ref(repo, actor, base_ref, &merge_oid, Some(base_oid))?;

        Ok(MergeOutcome {
            merge_oid,
            fast_forward,
        })
    }
}

/// Verify that `oid` resolves to a real commit object in the bare repository.
fn verify_commit(git_bin: &str, repo: &Repository, oid: &str) -> Result<()> {
    let spec = format!("{oid}^{{commit}}");
    let out = std::process::Command::new(git_bin)
        .args(["rev-parse", "--verify", "--quiet", &spec])
        .current_dir(&repo.path)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitdError::InvalidInput(format!(
            "oid is not a commit in this repository: {oid}"
        )))
    }
}

/// Compute the merged tree of `base_oid` and `head_oid`.
///
/// Uses a direct `Command` (not `run_capture`) because `git merge-tree
/// --write-tree` exits 1 on a conflict while still printing diagnostics; we must
/// read stdout on that path to distinguish a conflict from a hard failure.
fn merge_tree(git_bin: &str, repo: &Repository, base_oid: &str, head_oid: &str) -> Result<String> {
    let out = std::process::Command::new(git_bin)
        .args(["merge-tree", "--write-tree", base_oid, head_oid])
        .current_dir(&repo.path)
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first_line = stdout.lines().next().unwrap_or("").trim().to_string();
    if out.status.success() {
        if first_line.is_empty() {
            return Err(GitdError::GitCommandFailed {
                program: format!("{git_bin} merge-tree --write-tree"),
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            });
        }
        Ok(first_line)
    } else if out.status.code() == Some(1) {
        // On a conflict (exit 1), `merge-tree --write-tree` prints the conflicted
        // tree oid on line 1 and the human-readable `CONFLICT (...): Merge
        // conflict in <path>` diagnostics on later lines. Surface the named
        // conflict(s) when present rather than the bare tree oid, which is
        // meaningless to a caller.
        let detail = stdout
            .lines()
            .filter(|l| l.contains("CONFLICT"))
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join("; ");
        let detail = if detail.is_empty() {
            "merge conflict".to_string()
        } else {
            detail
        };
        Err(GitdError::MergeConflict(detail))
    } else {
        Err(GitdError::GitCommandFailed {
            program: format!("{git_bin} merge-tree --write-tree"),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })
    }
}

/// Create a two-parent merge commit for `tree_oid` with `base_oid`/`head_oid` as
/// parents. Uses a direct `Command` so committer identity is injected via the
/// environment, never depending on ambient `git config`.
fn commit_tree(
    git_bin: &str,
    repo: &Repository,
    tree_oid: &str,
    base_oid: &str,
    head_oid: &str,
    message: &str,
) -> Result<String> {
    let out = std::process::Command::new(git_bin)
        .env("GIT_AUTHOR_NAME", "jeryu-merge")
        .env("GIT_AUTHOR_EMAIL", "merge@jeryu.local")
        .env("GIT_COMMITTER_NAME", "jeryu-merge")
        .env("GIT_COMMITTER_EMAIL", "merge@jeryu.local")
        .args([
            "commit-tree",
            tree_oid,
            "-p",
            base_oid,
            "-p",
            head_oid,
            "-m",
            message,
        ])
        .current_dir(&repo.path)
        .output()?;
    if !out.status.success() {
        return Err(GitdError::GitCommandFailed {
            program: format!("{git_bin} commit-tree"),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    let oid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if oid.is_empty() {
        return Err(GitdError::GitCommandFailed {
            program: format!("{git_bin} commit-tree"),
            code: out.status.code(),
            stderr: "commit-tree produced no oid".to_string(),
        });
    }
    Ok(oid)
}

/// All-zero oid used by receive-pack for deletes.
pub const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// Validate a ref name using Git itself plus local sanity guards.
pub fn validate_ref_name(name: &str) -> Result<()> {
    if name.contains('\0') || name.starts_with('-') {
        return Err(GitdError::InvalidInput("invalid ref name".to_string()));
    }
    run_capture("git", &["check-ref-format", "--allow-onelevel", name], None).map(|_| ())
}

/// Whether an oid is all zeroes.
#[must_use]
pub fn is_zero_oid(oid: &str) -> bool {
    oid.len() == 40 && oid.chars().all(|c| c == '0')
}

fn force_update(
    manager: &RepoManager,
    repo: &Repository,
    name: &str,
    new_oid: &str,
    old_oid: Option<&str>,
    operation: RefOperation,
) -> Result<bool> {
    let Some(old_oid) = old_oid else {
        return Ok(false);
    };
    if operation != RefOperation::Update || is_zero_oid(old_oid) || is_zero_oid(new_oid) {
        return Ok(false);
    }
    if name.starts_with("refs/tags/") {
        return Ok(true);
    }
    if !name.starts_with("refs/heads/") {
        return Ok(false);
    }
    ObjectFsck::new(manager.config().git_bin.clone())
        .is_ancestor(repo, old_oid, new_oid)
        .map(|is_ancestor| !is_ancestor)
}
