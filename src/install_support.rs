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

#[path = "install_support_plan.rs"]
mod plan;

pub(crate) use plan::build_plan;

#[path = "install_support_render.rs"]
mod render;

pub(crate) use render::{prompt_for_confirmation, render_plan, version_hint};
