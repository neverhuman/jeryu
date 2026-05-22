use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::*;

pub(crate) fn plan_standard(
    spec: &StandardSpec,
    files: &[ManagedFile],
    opts: &RepoStandardOptions,
) -> Result<RepoStandardReport> {
    let changes = files
        .iter()
        .map(|file| {
            let path = spec.repo_root.join(file.path);
            let operation = match fs::read_to_string(&path) {
                Ok(existing) if existing == file.content => ManagedFileOperation::Unchanged,
                Ok(_) => ManagedFileOperation::Update,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    ManagedFileOperation::Create
                }
                Err(_) => ManagedFileOperation::Update,
            };
            ManagedFileChange {
                path: file.path.to_string(),
                operation,
                executable: file.executable,
                sha256: sha256_hex(file.content.as_bytes()),
            }
        })
        .collect();

    let actual_hooks = if spec.repo_root.join(".git").is_dir() {
        git_config_get(&spec.repo_root, "core.hooksPath")?
    } else {
        None
    };
    let hook_operation = if !opts.configure_git_hooks
        || actual_hooks.as_deref() == Some(".jeryu/hooks")
        || !spec.repo_root.join(".git").is_dir()
    {
        ManagedFileOperation::Unchanged
    } else if actual_hooks.is_some() {
        ManagedFileOperation::Update
    } else {
        ManagedFileOperation::Create
    };

    Ok(RepoStandardReport {
        status: RepoStandardStatus::Planned,
        repo_path: spec.repo_root.display().to_string(),
        standard_version: AGENT_FIRST_STANDARD_VERSION.to_string(),
        profile: spec.profile.clone(),
        provider: spec.provider,
        base_branch: spec.base_branch.clone(),
        repo_slug: spec.repo_slug.clone(),
        autonomy_dir: spec.autonomy_dir.clone(),
        required_check: REQUIRED_CHECK_NAME.to_string(),
        changes,
        hook_config: HookConfigChange {
            desired: ".jeryu/hooks".to_string(),
            actual: actual_hooks,
            operation: hook_operation,
        },
    })
}

pub(crate) fn apply_standard(repo_root: &Path, files: &[ManagedFile]) -> Result<()> {
    for file in files {
        let path = repo_root.join(file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        if fs::read_to_string(&path).ok().as_deref() != Some(file.content.as_str()) {
            fs::write(&path, &file.content)
                .with_context(|| format!("writing {}", path.display()))?;
        }
        set_executable(&path, file.executable)?;
    }
    Ok(())
}

pub(crate) fn report_is_clean(report: &RepoStandardReport) -> bool {
    report
        .changes
        .iter()
        .all(|change| change.operation == ManagedFileOperation::Unchanged)
        && report.hook_config.operation == ManagedFileOperation::Unchanged
}

pub(crate) fn print_report(report: &RepoStandardReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!(
        "jeryu standard {} for {} ({})",
        match report.status {
            RepoStandardStatus::Clean => "clean",
            RepoStandardStatus::Planned => "plan",
            RepoStandardStatus::Applied => "applied",
            RepoStandardStatus::Drift => "drift",
        },
        report.repo_slug,
        report.repo_path
    );
    for change in &report.changes {
        if change.operation != ManagedFileOperation::Unchanged {
            println!("  {:?}: {}", change.operation, change.path);
        }
    }
    if report.hook_config.operation != ManagedFileOperation::Unchanged {
        println!(
            "  {:?}: git core.hooksPath -> {}",
            report.hook_config.operation, report.hook_config.desired
        );
    }
    Ok(())
}
