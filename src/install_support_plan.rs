use super::*;

#[allow(clippy::too_many_arguments)] // install plan assembly is intentionally explicit so CLI output stays stable
pub(crate) fn build_plan(mode: &str, opts: &InstallOptions) -> InstallPlan {
    let prefix = opts.prefix.display().to_string();
    let target = install_target(&opts.prefix);
    let source = current_exe_string();
    let platform = detect_platform(&opts.prefix);
    let path_advice = if platform.in_path {
        None
    } else {
        let rc_file = shell_profile_path(platform.shell.as_deref());
        Some(PathAdvice {
            shell: platform.shell.clone(),
            rc_file: rc_file.as_ref().map(|path| path.display().to_string()),
            snippet: rc_file
                .as_ref()
                .map(|_| path_snippet(&opts.prefix, platform.shell.as_deref())),
            refresh_performed: matches!(opts.path_mode, PathMode::Refresh),
        })
    };
    let mut steps = vec![
        PlanStep {
            id: "ensure-prefix".into(),
            label: "ensure install prefix exists".into(),
            detail: format!("create {}", opts.prefix.display()),
            command: Some(format!("mkdir -p {}", opts.prefix.display())),
            requires_sudo: false,
            estimated_seconds: Some(1),
        },
        PlanStep {
            id: "install-binary".into(),
            label: "replace the binary atomically".into(),
            detail: format!("copy {} -> {}", source, target.display()),
            command: Some(format!(
                "install -m 0755 <current-exe> {}",
                target.display()
            )),
            requires_sudo: false,
            estimated_seconds: Some(2),
        },
    ];
    if !platform.in_path {
        let detail = match opts.path_mode {
            PathMode::Advise => "print shell-specific PATH advice".to_string(),
            PathMode::Refresh => "write the shell profile with a guarded PATH block".to_string(),
            PathMode::Skip => "skip PATH advice and leave shell profiles untouched".to_string(),
        };
        steps.push(PlanStep {
            id: "path".into(),
            label: "handle PATH visibility".into(),
            detail,
            command: Some(match opts.path_mode {
                PathMode::Advise => format!(
                    "echo {}",
                    path_snippet(&opts.prefix, platform.shell.as_deref())
                ),
                PathMode::Refresh => {
                    if let Some(rc) = shell_profile_path(platform.shell.as_deref()) {
                        format!("append {} to {}", opts.prefix.display(), rc.display())
                    } else {
                        "no supported shell profile found".into()
                    }
                }
                PathMode::Skip => "no PATH mutation".into(),
            }),
            requires_sudo: false,
            estimated_seconds: Some(1),
        });
    }
    steps.push(PlanStep {
        id: "verify".into(),
        label: "verify the installed binary".into(),
        detail: "run jeryu --version from the target binary".into(),
        command: Some(format!("{} --version", target.display())),
        requires_sudo: false,
        estimated_seconds: Some(1),
    });
    InstallPlan {
        action: "install".into(),
        mode: mode.into(),
        prefix,
        target_binary: target.display().to_string(),
        source_binary: source,
        platform,
        path_advice,
        dry_run: opts.dry_run,
        json: opts.json,
        color: opts.color,
        interactive: opts.interactive,
        path_mode: opts.path_mode,
        verbose: opts.verbose,
        install_deps: opts.install_deps,
        allow_sudo: opts.allow_sudo,
        steps,
    }
}
