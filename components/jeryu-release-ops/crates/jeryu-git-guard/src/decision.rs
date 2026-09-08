//! Top-level Git command dispatch and global-option validation.

use crate::error::{GitDecision, GitGuardError};
use crate::policy::{ALLOWED_LOCAL, INTEGRATION_DENY, NETWORK_DENY, WORKTREE_DENY};
use crate::subcommands::{
    branch_decision, checkout_decision, config_decision, reflog_decision, switch_decision,
    tag_decision,
};

/// The PURE verdict function — the single source of truth.
///
/// `argv` is the arguments passed to `git` (NOT including the `git` program name),
/// e.g. `["commit", "-m", "msg"]`. `assigned_branch` is the agent's pinned branch.
/// Deny-by-default: only an explicit allowlist passes.
#[must_use]
pub fn git_command_decision(argv: &[String], assigned_branch: &str) -> GitDecision {
    let rendered = render_command(argv);
    let mut idx = 0;
    while idx < argv.len() {
        let token = argv[idx].as_str();
        if !token.starts_with('-') {
            break;
        }
        if is_denied_global(token) {
            return GitDecision::Deny(GitGuardError::global_flag(rendered));
        }
        if global_takes_value(token) {
            idx += 2;
            continue;
        }
        if is_benign_global(token) {
            idx += 1;
            continue;
        }
        return GitDecision::Deny(GitGuardError::global_flag(rendered));
    }

    let Some(subcommand) = argv.get(idx) else {
        return GitDecision::Allow;
    };
    let subcommand = subcommand.as_str();
    let rest = &argv[idx + 1..];
    if NETWORK_DENY.contains(&subcommand) {
        return GitDecision::Deny(GitGuardError::network(rendered));
    }
    if WORKTREE_DENY.contains(&subcommand) {
        return GitDecision::Deny(GitGuardError::worktree(rendered));
    }
    if INTEGRATION_DENY.contains(&subcommand) {
        return GitDecision::Deny(GitGuardError::integration(rendered));
    }
    match subcommand {
        "commit" => commit_decision(rest, rendered),
        "config" => config_decision(rest, rendered),
        "branch" => branch_decision(rest, rendered),
        "checkout" => checkout_decision(rest, assigned_branch, rendered),
        "switch" => switch_decision(rest, assigned_branch, rendered),
        "tag" => tag_decision(rest, rendered),
        "reflog" => reflog_decision(rest, rendered),
        _ if ALLOWED_LOCAL.contains(&subcommand) => GitDecision::Allow,
        _ => GitDecision::Deny(GitGuardError::subcommand(rendered)),
    }
}

fn render_command(argv: &[String]) -> String {
    let mut rendered = String::from("git");
    for argument in argv {
        rendered.push(' ');
        rendered.push_str(argument);
    }
    rendered
}

fn commit_decision(rest: &[String], command: String) -> GitDecision {
    for token in rest {
        if token == "--" {
            break;
        }
        let denied = token == "--no-verify"
            || (token.starts_with('-') && !token.starts_with("--") && token.contains('n'));
        if denied {
            return GitDecision::Deny(GitGuardError::commit_unverified(command));
        }
    }
    GitDecision::Allow
}

/// The git subcommand in `argv`, skipping leading global options.
#[must_use]
pub fn leading_subcommand(argv: &[String]) -> Option<&str> {
    let mut idx = 0;
    while idx < argv.len() {
        let token = argv[idx].as_str();
        if !token.starts_with('-') {
            break;
        }
        if global_takes_value(token) {
            idx += 2;
            continue;
        }
        idx += 1;
    }
    argv.get(idx).map(String::as_str)
}

fn is_denied_global(token: &str) -> bool {
    const EXACT: &[&str] = &[
        "-c",
        "--config-env",
        "--namespace",
        "--bare",
        "--super-prefix",
    ];
    const PREFIX: &[&str] = &[
        "--config-env=",
        "--git-dir",
        "--work-tree",
        "--exec-path=",
        "--namespace=",
        "--super-prefix=",
    ];
    if EXACT.contains(&token) || token == "--git-dir" || token == "--work-tree" {
        return true;
    }
    PREFIX.iter().any(|prefix| token.starts_with(prefix))
}

fn global_takes_value(token: &str) -> bool {
    token == "-C"
}

fn is_benign_global(token: &str) -> bool {
    const ALLOWED: &[&str] = &[
        "--no-pager",
        "--paginate",
        "-p",
        "--no-replace-objects",
        "--literal-pathspecs",
        "--no-literal-pathspecs",
        "--glob-pathspecs",
        "--noglob-pathspecs",
        "--icase-pathspecs",
        "--no-optional-locks",
        "--html-path",
        "--man-path",
        "--info-path",
        "--exec-path",
        "--version",
        "--help",
        "-P",
    ];
    ALLOWED.contains(&token)
}
