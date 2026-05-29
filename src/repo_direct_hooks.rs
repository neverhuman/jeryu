use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use super::{HookMode, HookProfile, RepoMode};

pub(crate) fn configure_hook_mode(
    repo_root: &Path,
    mode: HookMode,
    profile: HookProfile,
) -> Result<()> {
    match mode {
        HookMode::Off => {
            super::unset_git_config(repo_root, "core.hooksPath")?;
        }
        HookMode::Advisory | HookMode::Enforce => {
            let hooks_dir = repo_root.join(".jeryu/hooks");
            fs::create_dir_all(&hooks_dir)?;
            if matches!(profile, HookProfile::PrePush | HookProfile::All) {
                write_executable_hook(&hooks_dir.join("pre-push"), &pre_push_hook(mode))?;
            }
            if matches!(profile, HookProfile::PreCommitJankurai | HookProfile::All) {
                write_executable_hook(
                    &hooks_dir.join("pre-commit"),
                    &jankurai_pre_commit_hook(mode),
                )?;
            }
            super::run_git(
                repo_root,
                &["config", "--local", "core.hooksPath", ".jeryu/hooks"],
            )?;
        }
    }
    Ok(())
}

fn pre_push_hook(mode: HookMode) -> String {
    let blocking = matches!(mode, HookMode::Enforce);
    format!(
        "#!/usr/bin/env bash\nset -u\nREPO_ROOT=\"$(git rev-parse --show-toplevel)\"\ncd \"$REPO_ROOT\"\nQUALITY_GATES_SCRIPT=\"${{JERYU_PRE_PUSH_QUALITY_GATES:-$REPO_ROOT/ops/ci/quality-gates.sh}}\"\nPROTECTED_BRANCH=\"${{JERYU_PROTECTED_BRANCH:-main}}\"\nZERO_SHA=\"0000000000000000000000000000000000000000\"\nupdates=()\nwhile true; do\n  local_ref=\"\"\n  local_sha=\"\"\n  remote_ref=\"\"\n  remote_sha=\"\"\n  if ! IFS=' ' read -r local_ref local_sha remote_ref remote_sha; then\n    if [ -z \"${{local_ref}}${{local_sha}}${{remote_ref}}${{remote_sha}}\" ]; then\n      break\n    fi\n  fi\n  if [ \"${{remote_ref:-}}\" = \"refs/heads/$PROTECTED_BRANCH\" ]; then\n    echo \"error: direct pushes to $PROTECTED_BRANCH are blocked.\" >&2\n    exit 1\n  fi\n  if [ \"${{local_sha:-$ZERO_SHA}}\" != \"$ZERO_SHA\" ]; then\n    updates+=(\"$local_sha\")\n  fi\ndone\nif [ \"${{#updates[@]}}\" -gt 0 ]; then\n  git fetch --quiet origin \"$PROTECTED_BRANCH\"\n  BASE_REF=\"refs/remotes/origin/$PROTECTED_BRANCH\"\n  for local_sha in \"${{updates[@]}}\"; do\n    if ! git merge-base --is-ancestor \"$BASE_REF\" \"$local_sha\"; then\n      echo \"error: branch is not rebased on $BASE_REF; run git fetch origin $PROTECTED_BRANCH && git rebase $BASE_REF\" >&2\n      {}\n    fi\n    if git rev-list --merges \"$BASE_REF..$local_sha\" | grep -q .; then\n      echo \"error: merge commits after $BASE_REF are blocked; keep history linear with rebase\" >&2\n      {}\n    fi\n  done\nfi\nbash \"$QUALITY_GATES_SCRIPT\"\nstatus=$?\nif [ \"$status\" -ne 0 ]; then\n  echo \"jeryu advisory pre-push failed\" >&2\n  {}\nfi\nexit 0\n",
        if blocking { "exit 1" } else { "break" },
        if blocking { "exit 1" } else { "break" },
        if blocking { "exit $status" } else { "exit 0" }
    )
}

fn jankurai_pre_commit_hook(mode: HookMode) -> String {
    let blocking = matches!(mode, HookMode::Enforce);
    format!(
        "#!/usr/bin/env bash\nset -u\nPROTECTED_BRANCH=\"${{JERYU_PROTECTED_BRANCH:-main}}\"\ncurrent_branch=\"$(git rev-parse --abbrev-ref HEAD)\"\nif [ \"$current_branch\" = \"$PROTECTED_BRANCH\" ]; then\n  echo \"jeryu: direct commits on $PROTECTED_BRANCH are blocked; branch first and submit an MR\" >&2\n  {}\nfi\nbase=${{JERYU_JANKURAI_CHANGED_FROM:-origin/$PROTECTED_BRANCH}}\nif git rev-parse --verify \"$base\" >/dev/null 2>&1; then\n  if ! git merge-base --is-ancestor \"$base\" HEAD; then\n    echo \"jeryu: branch is not rebased on $base; rebase before submitting an MR\" >&2\n    {}\n  fi\n  if git rev-list --merges \"$base..HEAD\" | grep -q .; then\n    echo \"jeryu: merge commits after $base are blocked; keep history linear with rebase\" >&2\n    {}\n  fi\nelse\n  echo \"jeryu: $base is unavailable; run git fetch origin $PROTECTED_BRANCH before submitting an MR\" >&2\n  {}\nfi\nmkdir -p target/jankurai\njankurai audit . --changed-fast --changed-from \"$base\" --mode advisory --json target/jankurai/pre-commit-changed-fast.json --md target/jankurai/pre-commit-changed-fast.md\nstatus=$?\nif [ \"$status\" -ne 0 ]; then\n  echo \"jankurai changed-fast guard reported findings\" >&2\n  {}\nfi\nexit 0\n",
        if blocking { "exit 1" } else { ":" },
        if blocking { "exit 1" } else { ":" },
        if blocking { "exit 1" } else { ":" },
        if blocking { "exit 1" } else { ":" },
        if blocking { "exit $status" } else { "exit 0" }
    )
}

fn write_executable_hook(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

pub(crate) fn configure_git_hooks(repo_root: &Path) -> Result<()> {
    let hooks_dir = repo_root.join("ops/git-hooks");
    let pre_push = hooks_dir.join("pre-push");
    if !pre_push.is_file() {
        bail!("repo-managed hook is missing: {}", pre_push.display());
    }

    let output = super::git_output(
        repo_root,
        &["config", "--local", "core.hooksPath", "ops/git-hooks"],
    )
    .with_context(|| "configuring repo-managed git hooks".to_string())?;

    if !output.status.success() {
        bail!(
            "failed to configure core.hooksPath: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

pub(crate) fn mode_label(mode: RepoMode) -> &'static str {
    match mode {
        RepoMode::Direct => "direct",
        RepoMode::Observed => "observed",
        RepoMode::Enforced => "enforced",
    }
}

pub(crate) fn hook_label(mode: HookMode) -> &'static str {
    match mode {
        HookMode::Off => "off",
        HookMode::Advisory => "advisory",
        HookMode::Enforce => "enforce",
    }
}
