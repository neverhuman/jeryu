//! Owner: Git mirror helper
//! Proof: `cargo test -p jeryu -- git_mirror`
//! Invariants: Mirror failures are recorded and only become fatal in strict mode.

use anyhow::Result;
use std::path::Path;

use crate::git::system::SystemGit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushMirrorPlan {
    pub remote_name: String,
    pub git_args: Vec<String>,
    pub ref_name: Option<String>,
}

pub fn parse_push_mirror_plan(argv: &[String], remote_name: &str) -> Option<PushMirrorPlan> {
    if !matches!(argv.first().map(String::as_str), Some("push")) {
        return None;
    }

    let mut positionals = Vec::new();
    let mut pass_flags = Vec::new();
    let mut i = 1;
    while i < argv.len() {
        let arg = &argv[i];
        if arg == "--" {
            positionals.extend(argv[i + 1..].iter().cloned());
            break;
        }

        if arg == "--tags" || arg == "--all" || arg == "--mirror" {
            pass_flags.push(arg.clone());
            i += 1;
            continue;
        }

        if arg == "-f" || arg == "--force" || arg.starts_with("--force-with-lease") {
            pass_flags.push(arg.clone());
            i += 1;
            continue;
        }

        if arg == "-u" || arg == "--set-upstream" {
            i += 1;
            continue;
        }

        if option_consumes_next(arg) {
            i += 2;
            continue;
        }

        if arg.starts_with('-') {
            i += 1;
            continue;
        }

        positionals.push(arg.clone());
        i += 1;
    }

    let refspecs = push_refspecs(&positionals);
    let mut git_args = vec!["push".to_string()];
    git_args.extend(pass_flags.iter().cloned());
    git_args.push(remote_name.to_string());
    if !pass_flags
        .iter()
        .any(|flag| matches!(flag.as_str(), "--all" | "--tags" | "--mirror"))
    {
        if refspecs.is_empty() {
            git_args.push("HEAD".to_string());
        } else {
            git_args.extend(refspecs.iter().cloned());
        }
    }

    Some(PushMirrorPlan {
        remote_name: remote_name.to_string(),
        git_args,
        ref_name: Some(match refspecs.first().cloned() {
            Some(value) => value,
            None => "HEAD".to_string(),
        }),
    })
}

pub fn mirror_push_plan(cwd: &Path, plan: &PushMirrorPlan) -> Result<bool> {
    let git = SystemGit::resolve()?;
    let args: Vec<&str> = plan.git_args.iter().map(String::as_str).collect();
    let status = git.status(cwd, &args)?;
    Ok(status.success())
}

pub fn mirror_push(
    cwd: &Path,
    remote_name: &str,
    branch: Option<&str>,
    mirror: bool,
) -> Result<bool> {
    let mut args = vec!["push".to_string()];
    if mirror {
        args.push("--mirror".to_string());
        args.push(remote_name.to_string());
    } else {
        args.push(remote_name.to_string());
        args.push(match branch {
            Some(name) => name.to_string(),
            None => "HEAD".to_string(),
        });
    }
    mirror_push_plan(
        cwd,
        &PushMirrorPlan {
            remote_name: remote_name.to_string(),
            git_args: args,
            ref_name: Some(match branch {
                Some(name) => name.to_string(),
                None => "HEAD".into(),
            }),
        },
    )
}

fn option_consumes_next(arg: &str) -> bool {
    matches!(
        arg,
        "--repo" | "--receive-pack" | "--exec" | "-o" | "--push-option"
    )
}

fn push_refspecs(positionals: &[String]) -> Vec<String> {
    match positionals {
        [] => Vec::new(),
        [single] if looks_like_refspec(single) => vec![single.clone()],
        [_remote] => Vec::new(),
        [_remote, rest @ ..] => rest.to_vec(),
    }
}

fn looks_like_refspec(value: &str) -> bool {
    value == "HEAD" || value.contains(':') || value.starts_with("refs/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn push_without_args_mirrors_head() {
        let plan = parse_push_mirror_plan(&argv(&["push"]), "jeryu").unwrap();
        assert_eq!(plan.git_args, argv(&["push", "jeryu", "HEAD"]));
        assert_eq!(plan.ref_name.as_deref(), Some("HEAD"));
    }

    #[test]
    fn push_origin_head_mirrors_head_to_jeryu() {
        let plan = parse_push_mirror_plan(&argv(&["push", "origin", "HEAD"]), "jeryu").unwrap();
        assert_eq!(plan.git_args, argv(&["push", "jeryu", "HEAD"]));
    }

    #[test]
    fn set_upstream_option_ordering_keeps_refspec() {
        let plan = parse_push_mirror_plan(
            &argv(&["push", "--set-upstream", "origin", "main"]),
            "mirror",
        )
        .unwrap();
        assert_eq!(plan.git_args, argv(&["push", "mirror", "main"]));
        let plan = parse_push_mirror_plan(
            &argv(&["push", "origin", "--set-upstream", "main"]),
            "mirror",
        )
        .unwrap();
        assert_eq!(plan.git_args, argv(&["push", "mirror", "main"]));
    }

    #[test]
    fn tags_all_and_refspecs_are_preserved() {
        let tags = parse_push_mirror_plan(&argv(&["push", "--tags", "origin"]), "jeryu").unwrap();
        assert_eq!(tags.git_args, argv(&["push", "--tags", "jeryu"]));
        let all = parse_push_mirror_plan(&argv(&["push", "origin", "--all"]), "jeryu").unwrap();
        assert_eq!(all.git_args, argv(&["push", "--all", "jeryu"]));
        let refspec =
            parse_push_mirror_plan(&argv(&["push", "origin", "main:main"]), "jeryu").unwrap();
        assert_eq!(refspec.git_args, argv(&["push", "jeryu", "main:main"]));
    }
}
