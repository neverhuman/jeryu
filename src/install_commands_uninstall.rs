use super::*;

pub(crate) async fn uninstall(opts: &InstallOptions) -> Result<i32> {
    let target = install_target(&opts.prefix);
    let backup_prefix = opts.prefix.join(".jeryu-backups");
    let shell = env::var("SHELL").ok();
    let rc_path = shell_profile_path(shell.as_deref());
    let mut report = UninstallReport {
        action: "uninstall".into(),
        prefix: opts.prefix.display().to_string(),
        binary: target.display().to_string(),
        backup_dir: backup_prefix.display().to_string(),
        dry_run: opts.dry_run,
        path_mode: opts.path_mode,
        path_rc_file: rc_path.as_ref().map(|path| path.display().to_string()),
        binary_present_before: target.exists(),
        backups_present_before: backup_prefix.exists(),
        path_block_found: install_runtime::path_block_found(rc_path.as_deref()),
        binary_removed: false,
        backups_removed: false,
        path_block_removed: false,
    };

    if opts.dry_run {
        emit_uninstall_report(&report, opts)?;
        return Ok(0);
    }

    if report.binary_present_before {
        fs::remove_file(&target).with_context(|| format!("removing {}", target.display()))?;
        report.binary_removed = true;
    }
    if report.backups_present_before {
        fs::remove_dir_all(&backup_prefix)
            .with_context(|| format!("removing {}", backup_prefix.display()))?;
        report.backups_removed = true;
    }
    if matches!(opts.path_mode, PathMode::Refresh) {
        report.path_block_removed =
            install_runtime::remove_shell_profile_path_block(shell.as_deref())?;
        report.path_block_found |= report.path_block_removed;
    }

    emit_uninstall_report(&report, opts)?;
    Ok(0)
}

fn emit_uninstall_report(report: &UninstallReport, opts: &InstallOptions) -> Result<()> {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    let color = should_colorize(opts.color, opts.json);
    let label = if opts.dry_run {
        status_label(color, "PLAN", "36;1")
    } else {
        status_label(color, "OK", "32;1")
    };
    println!("{} JeRyu uninstall", label);
    println!("  binary:  {}", report.binary);
    println!(
        "  action:  {}",
        if opts.dry_run {
            if report.binary_present_before {
                "would remove binary"
            } else {
                "binary not present"
            }
        } else if report.binary_removed {
            "removed binary"
        } else {
            "binary not present"
        }
    );
    println!(
        "  backups: {}",
        if opts.dry_run {
            if report.backups_present_before {
                "would remove installer backups"
            } else {
                "none found"
            }
        } else if report.backups_removed {
            "removed installer backups"
        } else {
            "none found"
        }
    );

    match report.path_rc_file.as_deref() {
        Some(rc) if report.path_block_removed => {
            println!("  PATH:    removed guarded block from {rc}");
        }
        Some(rc) if report.path_block_found && matches!(opts.path_mode, PathMode::Refresh) => {
            println!("  PATH:    guarded block was found but could not be removed from {rc}");
        }
        Some(rc) if report.path_block_found && matches!(opts.path_mode, PathMode::Skip) => {
            println!("  PATH:    guarded block left in {rc} (--path-mode skip)");
        }
        Some(rc) if report.path_block_found => {
            println!(
                "  PATH:    guarded block remains in {rc}; rerun uninstall with PATH block write enabled to remove it"
            );
        }
        Some(rc) => {
            println!("  PATH:    no guarded block found in {rc}");
        }
        None => {
            println!("  PATH:    no supported shell profile detected");
        }
    }
    Ok(())
}
