//! Owner: Local installer and guided bootstrap UX
//! Proof: `cargo test -p jeryu -- install`
//! Invariants: Local installs remain user-space by default, avoid shell mutations unless requested, and never require sudo for the default path.

use super::*;

pub(crate) fn current_exe_string() -> String {
    match env::current_exe() {
        Ok(path) => path.display().to_string(),
        Err(_) => "(unavailable)".into(),
    }
}

pub(crate) fn install_target(prefix: &Path) -> PathBuf {
    prefix.join("jeryu")
}

pub(crate) fn detect_platform(prefix: &Path) -> PlatformProbe {
    let shell = env::var("SHELL").ok();
    PlatformProbe {
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        shell,
        tty: io::stdout().is_terminal(),
        in_path: path_contains_dir(prefix),
    }
}

pub(crate) fn path_contains_dir(dir: &Path) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|entry| entry == dir)
}

pub(crate) fn shell_profile_path(shell: Option<&str>) -> Option<PathBuf> {
    let shell = shell?;
    let name = Path::new(shell)
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let home = dirs::home_dir()?;
    match name.as_str() {
        "bash" => Some(home.join(".bashrc")),
        "zsh" => Some(home.join(".zshrc")),
        "fish" => Some(home.join(".config/fish/config.fish")),
        _ => None,
    }
}

pub(crate) fn path_snippet(prefix: &Path, shell: Option<&str>) -> String {
    let path = prefix.display();
    let shell_name = match shell {
        Some(value) => match Path::new(value).file_name() {
            Some(name) => name.to_string_lossy().to_ascii_lowercase(),
            None => String::new(),
        },
        None => String::new(),
    };
    match shell_name.as_str() {
        "fish" => format!(
            "{JERYU_PATH_START}\nset -gx PATH \"{}\" $PATH\n{JERYU_PATH_END}",
            path
        ),
        _ => format!(
            "{JERYU_PATH_START}\nexport PATH=\"{}:$PATH\"\n{JERYU_PATH_END}",
            path
        ),
    }
}

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

pub(crate) fn should_colorize(mode: ColorMode, json: bool) -> bool {
    if json {
        return false;
    }
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none(),
    }
}

pub(crate) fn should_interactive(mode: InteractiveMode) -> bool {
    match mode {
        InteractiveMode::Always => true,
        InteractiveMode::Never => false,
        InteractiveMode::Auto => io::stdin().is_terminal(),
    }
}

pub(crate) fn color_text(enabled: bool, code: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub(crate) fn status_label(enabled: bool, label: &str, code: &str) -> String {
    format!("[{}]", color_text(enabled, code, label))
}

#[allow(clippy::too_many_arguments)] // install renderer: closures + style codes inline by design; struct-wrap is tracked as a follow-up
pub(crate) fn render_plan_steps<T, FReq, FLabel, FDetail, FCommand>(
    steps: &[T],
    verbose: bool,
    mut requires_highlight: FReq,
    mut label_of: FLabel,
    mut detail_of: FDetail,
    mut command_of: FCommand,
    enabled: bool,
    label_when_true: &str,
    label_when_false: &str,
    true_code: &str,
    false_code: &str,
) where
    FReq: FnMut(&T) -> bool,
    FLabel: FnMut(&T) -> &str,
    FDetail: FnMut(&T) -> &str,
    FCommand: FnMut(&T) -> Option<&str>,
{
    for step in steps {
        let label = if requires_highlight(step) {
            status_label(enabled, label_when_true, true_code)
        } else {
            status_label(enabled, label_when_false, false_code)
        };
        println!("  {} {} - {}", label, label_of(step), detail_of(step));
        if verbose && let Some(command) = command_of(step) {
            println!("      {}", command);
        }
    }
}

pub(crate) fn prompt_for_confirmation_with_message(
    prompt: &str,
    refusal_message: &str,
    interactive: InteractiveMode,
    yes: bool,
) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if !should_interactive(interactive) {
        bail!("{}", refusal_message);
    }
    print!("{}", prompt);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading confirmation")?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub(crate) fn render_plan(plan: &InstallPlan) {
    let color = should_colorize(plan.color, plan.json);
    println!(
        "{} {}",
        status_label(color, "PLAN", "36;1"),
        color_text(color, "1", &format!("JeRyu {} plan", plan.mode))
    );
    println!("  prefix: {}", plan.prefix);
    println!("  target: {}", plan.target_binary);
    println!("  source: {}", plan.source_binary);
    println!(
        "  platform: {} / {}{}",
        plan.platform.os,
        plan.platform.arch,
        if plan.platform.tty { " / tty" } else { "" }
    );
    println!(
        "  PATH: {}",
        if plan.platform.in_path {
            "already on PATH"
        } else {
            "not on PATH"
        }
    );
    render_plan_steps(
        &plan.steps,
        plan.verbose,
        |step| step.requires_sudo,
        |step| step.label.as_str(),
        |step| step.detail.as_str(),
        |step| step.command.as_deref(),
        color,
        "WARN",
        "RUN",
        "33;1",
        "36;1",
    );
    if let Some(advice) = &plan.path_advice {
        match plan.path_mode {
            PathMode::Skip => {
                println!("  PATH: skipped by request");
            }
            PathMode::Advise | PathMode::Refresh => {
                if let Some(snippet) = &advice.snippet {
                    println!("  PATH snippet:");
                    for line in snippet.lines() {
                        println!("      {}", line);
                    }
                }
            }
        }
    }
}

pub(crate) fn prompt_for_confirmation(_plan: &InstallPlan, opts: &InstallOptions) -> Result<bool> {
    prompt_for_confirmation_with_message(
        "Proceed with this install? [y/N] ",
        "refusing to mutate the machine without --yes in non-interactive mode; rerun with --yes or --dry-run",
        opts.interactive,
        opts.yes,
    )
}

pub(crate) fn version_hint(binary: &Path) -> String {
    format!("Try: {} --version", binary.display())
}
