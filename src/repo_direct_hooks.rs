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
        "#!/bin/sh\nset -u\nREPO_ROOT=\"$(git rev-parse --show-toplevel)\"\ncd \"$REPO_ROOT\"\nbash ops/ci/quality-gates.sh\nstatus=$?\nif [ \"$status\" -ne 0 ]; then\n  echo \"jeryu advisory pre-push failed\" >&2\n  {}\nfi\nexit 0\n",
        if blocking { "exit $status" } else { "exit 0" }
    )
}

fn jankurai_pre_commit_hook(mode: HookMode) -> String {
    let blocking = matches!(mode, HookMode::Enforce);
    format!(
        "#!/bin/sh\nset -u\nmkdir -p target/jankurai\nbase=${{JERYU_JANKURAI_CHANGED_FROM:-origin/main}}\njankurai audit . --changed-fast --changed-from \"$base\" --mode advisory --json target/jankurai/pre-commit-changed-fast.json --md target/jankurai/pre-commit-changed-fast.md\nstatus=$?\nif [ \"$status\" -ne 0 ]; then\n  echo \"jankurai changed-fast guard reported findings\" >&2\n  {}\nfi\nexit 0\n",
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

    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "core.hooksPath", "ops/git-hooks"])
        .output()
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
