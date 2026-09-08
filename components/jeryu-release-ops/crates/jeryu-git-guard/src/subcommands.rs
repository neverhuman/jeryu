//! Validation for local Git subcommands that have both read and mutation forms.

use crate::error::{GitDecision, GitGuardError};

pub(crate) fn config_decision(args: &[String], command: String) -> GitDecision {
    const WRITE_FLAGS: &[&str] = &[
        "--add",
        "--unset",
        "--unset-all",
        "--replace-all",
        "--edit",
        "-e",
        "--rename-section",
        "--remove-section",
    ];
    let mut positionals = 0;
    for argument in args {
        if WRITE_FLAGS.contains(&argument.as_str()) {
            return GitDecision::Deny(GitGuardError::config_write(command));
        }
        if !argument.starts_with('-') {
            positionals += 1;
        }
    }
    if positionals >= 2 {
        return GitDecision::Deny(GitGuardError::config_write(command));
    }
    GitDecision::Allow
}

pub(crate) fn branch_decision(args: &[String], command: String) -> GitDecision {
    const MUTATION: &[&str] = &[
        "-d",
        "-D",
        "--delete",
        "-m",
        "-M",
        "--move",
        "-c",
        "-C",
        "--copy",
        "-f",
        "--force",
        "--set-upstream-to",
        "-u",
        "--unset-upstream",
        "--edit-description",
    ];
    const VALUE_FLAGS: &[&str] = &[
        "--contains",
        "--no-contains",
        "--merged",
        "--no-merged",
        "--points-at",
        "--format",
        "--sort",
        "--color",
        "--abbrev",
    ];
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        if MUTATION.contains(&argument) {
            return GitDecision::Deny(GitGuardError::branch(command));
        }
        if VALUE_FLAGS.contains(&argument) {
            index += 2;
            continue;
        }
        if argument.starts_with('-') {
            index += 1;
            continue;
        }
        return GitDecision::Deny(GitGuardError::branch(command));
    }
    GitDecision::Allow
}

pub(crate) fn checkout_decision(args: &[String], assigned: &str, command: String) -> GitDecision {
    const CREATE: &[&str] = &["-b", "-B", "--orphan", "--detach"];
    let mut positionals = Vec::new();
    let mut saw_dashdash = false;
    for argument in args {
        if argument == "--" {
            saw_dashdash = true;
            break;
        }
        if CREATE.contains(&argument.as_str()) {
            return GitDecision::Deny(GitGuardError::branch(command));
        }
        if !argument.starts_with('-') || argument == "-" {
            positionals.push(argument.as_str());
        }
    }
    if saw_dashdash {
        return GitDecision::Allow;
    }
    let safe = positionals.is_empty()
        || positionals
            .iter()
            .all(|path| *path == "." || *path == assigned);
    if safe {
        GitDecision::Allow
    } else {
        GitDecision::Deny(GitGuardError::branch(command))
    }
}

pub(crate) fn switch_decision(args: &[String], assigned: &str, command: String) -> GitDecision {
    const CREATE: &[&str] = &["-c", "-C", "--create", "--orphan", "--detach"];
    let mut positionals = Vec::new();
    for argument in args {
        if CREATE.contains(&argument.as_str()) {
            return GitDecision::Deny(GitGuardError::branch(command));
        }
        if !argument.starts_with('-') || argument == "-" {
            positionals.push(argument.as_str());
        }
    }
    if positionals.is_empty() || positionals.iter().all(|path| *path == assigned) {
        GitDecision::Allow
    } else {
        GitDecision::Deny(GitGuardError::branch(command))
    }
}

pub(crate) fn tag_decision(args: &[String], command: String) -> GitDecision {
    const CREATE: &[&str] = &[
        "-a",
        "--annotate",
        "-s",
        "--sign",
        "-d",
        "--delete",
        "-m",
        "--message",
        "-f",
        "--force",
        "-u",
        "--local-user",
        "-F",
        "--file",
        "--create-reflog",
    ];
    const LIST_FLAGS: &[&str] = &[
        "-l",
        "--list",
        "-n",
        "--contains",
        "--no-contains",
        "--points-at",
        "--merged",
        "--no-merged",
        "--sort",
        "--format",
        "--color",
    ];
    let mut has_list_flag = false;
    let mut has_positional = false;
    for argument in args {
        if CREATE.contains(&argument.as_str()) {
            return GitDecision::Deny(GitGuardError::branch(command));
        }
        if LIST_FLAGS.contains(&argument.as_str()) {
            has_list_flag = true;
        } else if !argument.starts_with('-') {
            has_positional = true;
        }
    }
    if has_positional && !has_list_flag {
        return GitDecision::Deny(GitGuardError::branch(command));
    }
    GitDecision::Allow
}

pub(crate) fn reflog_decision(args: &[String], command: String) -> GitDecision {
    match args.first().map(String::as_str) {
        Some("expire") | Some("delete") => GitDecision::Deny(GitGuardError::integration(command)),
        _ => GitDecision::Allow,
    }
}
