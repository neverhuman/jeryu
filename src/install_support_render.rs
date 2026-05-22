use super::*;

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
