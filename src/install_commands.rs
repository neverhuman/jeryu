use super::*;

pub(crate) async fn install_local(opts: &InstallOptions) -> Result<i32> {
    let plan = build_plan("local", opts);
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_plan(&plan);
    }
    if opts.dry_run {
        return Ok(0);
    }

    if matches!(opts.path_mode, PathMode::Refresh)
        && !plan.platform.in_path
        && shell_profile_path(plan.platform.shell.as_deref()).is_none()
    {
        bail!("PATH block write requires a supported shell profile (bash, zsh, or fish)");
    }
    if !prompt_for_confirmation(&plan, opts)? {
        bail!("install cancelled");
    }

    let step_started = Instant::now();
    install_runtime::install_binary(&opts.prefix).await?;
    if matches!(opts.path_mode, PathMode::Refresh) {
        install_runtime::refresh_shell_profile(&opts.prefix, plan.platform.shell.as_deref())?;
    }
    install_runtime::verify_binary(&install_target(&opts.prefix)).await?;
    if !plan.platform.in_path
        && matches!(opts.path_mode, PathMode::Advise)
        && let Some(advice) = &plan.path_advice
    {
        if let Some(rc) = &advice.rc_file {
            println!("PATH advice: add {} to {}", opts.prefix.display(), rc);
        }
        if let Some(snippet) = &advice.snippet {
            println!("{snippet}");
        }
    }
    println!(
        "{} installed jeryu to {} in {}s",
        status_label(should_colorize(opts.color, opts.json), "OK", "32;1"),
        install_target(&opts.prefix).display(),
        step_started.elapsed().as_secs_f32()
    );
    Ok(0)
}

pub(crate) async fn doctor(opts: &InstallOptions) -> Result<i32> {
    let target = install_target(&opts.prefix);
    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => PathBuf::from("(unavailable)"),
    };
    let version = match install_runtime::run_output(&target, &["--version"]).await {
        Ok(output) => Some(output.trim().to_string()),
        Err(_) => None,
    };
    let report = DoctorReport {
        prefix: opts.prefix.display().to_string(),
        binary: target.display().to_string(),
        current_exe: current_exe.display().to_string(),
        installed: target.exists(),
        version_ok: version.is_some(),
        version_output: version,
    };
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("JeRyu install doctor");
        println!("  prefix:       {}", report.prefix);
        println!("  binary:       {}", report.binary);
        println!("  current exe:  {}", report.current_exe);
        println!("  installed:    {}", report.installed);
        println!("  version ok:   {}", report.version_ok);
        if let Some(output) = &report.version_output {
            println!("  version:      {}", output);
        }
    }
    if !report.installed {
        bail!("installed binary not found: {}", report.binary);
    }
    if !report.version_ok {
        bail!(
            "installed binary did not respond to --version: {}",
            version_hint(&target)
        );
    }
    Ok(0)
}

pub(crate) async fn smoke(opts: &InstallOptions) -> Result<i32> {
    let tmp = tempfile::tempdir().context("creating smoke scratch dir")?;
    let smoke_opts = InstallOptions {
        prefix: tmp.path().to_path_buf(),
        dry_run: opts.dry_run,
        json: opts.json,
        yes: opts.yes,
        color: opts.color,
        interactive: opts.interactive,
        path_mode: opts.path_mode,
        verbose: opts.verbose,
        install_deps: false,
        allow_sudo: false,
    };
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "smoke",
                "prefix": smoke_opts.prefix,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("JeRyu install smoke");
    }
    if opts.dry_run {
        return Ok(0);
    }
    if !prompt_for_confirmation(&build_plan("smoke", &smoke_opts), &smoke_opts)? {
        bail!("smoke install cancelled");
    }
    install_runtime::install_binary(&smoke_opts.prefix).await?;
    install_runtime::verify_binary(&install_target(&smoke_opts.prefix)).await?;
    Ok(0)
}

pub(crate) async fn server(opts: &InstallOptions) -> Result<i32> {
    let prefix = &opts.prefix;
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "server",
                "prefix": prefix,
                "dry_run": opts.dry_run,
                "install_deps": opts.install_deps,
                "allow_sudo": opts.allow_sudo,
            }))?
        );
    } else {
        println!("JeRyu server setup");
    }
    if opts.dry_run {
        return Ok(0);
    }

    if !prompt_for_confirmation(&build_plan("server", opts), opts)? {
        bail!("server setup cancelled");
    }
    install_runtime::install_binary(prefix).await?;
    install_runtime::ensure_docker(opts).await?;
    install_runtime::run_installed_binary(&install_target(prefix), &["init"]).await?;
    Ok(0)
}

#[path = "install_commands_uninstall.rs"]
mod uninstall;
pub(crate) use uninstall::*;

#[path = "install_commands_guided.rs"]
mod guided;
pub(crate) use guided::*;

#[cfg(test)]
mod install_commands_tests;
