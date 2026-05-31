//! Adapter for `jeryu onboard`.
//!
//! Rehearses onboarding an existing checkout onto a jeryu forge by printing the
//! ordered plan. Dry-run only for now: the live server transport is not wired,
//! so this never mutates the checkout or contacts a server. The plan is pure and
//! deterministic so it is snapshot-testable.

use std::io::Write;

use serde::Serialize;

use crate::cli::OnboardArgs;
use crate::client::{ClientError, ClientResult};
use crate::commands::render;

/// One ordered onboarding step.
#[derive(Debug, Serialize)]
struct PlanStep {
    order: u32,
    action: String,
    detail: String,
}

/// The full onboarding plan emitted under `--json`.
#[derive(Debug, Serialize)]
struct OnboardPlan {
    path: String,
    repo: String,
    owner: String,
    host: String,
    remote_url: String,
    dry_run: bool,
    steps: Vec<PlanStep>,
}

pub(crate) fn run(json: bool, args: OnboardArgs, out: &mut dyn Write) -> ClientResult<()> {
    // The transport is not live; refuse anything but the dry-run rehearsal so a
    // caller never believes the checkout was actually repointed/registered.
    if !args.dry_run {
        return Err(ClientError::NotWired(
            "onboard server transport is not live; re-run with --dry-run".into(),
        ));
    }

    let repo = repo_name(&args.path)?;
    let remote_url = format!(
        "{}/{}/{}.git",
        args.host.trim_end_matches('/'),
        args.owner,
        repo
    );

    let steps = vec![
        PlanStep {
            order: 1,
            action: "create".into(),
            detail: format!("create repository {}/{} on {}", args.owner, repo, args.host),
        },
        PlanStep {
            order: 2,
            action: "materialize".into(),
            detail: format!(
                "push the {} working tree to the server-side repository",
                repo
            ),
        },
        PlanStep {
            order: 3,
            action: "repoint-remote".into(),
            detail: format!("set origin -> {remote_url}"),
        },
        PlanStep {
            order: 4,
            action: "register".into(),
            detail: format!(
                "register {}/{} with the forge control plane",
                args.owner, repo
            ),
        },
        PlanStep {
            order: 5,
            action: "set-autonomy".into(),
            detail: "apply the full-auto autonomy profile (R0-R4 auto, R5 fail-closed)".into(),
        },
    ];

    let plan = OnboardPlan {
        path: args.path.clone(),
        repo: repo.clone(),
        owner: args.owner.clone(),
        host: args.host.clone(),
        remote_url,
        dry_run: true,
        steps,
    };

    if json {
        return render(out, true, &plan, "");
    }

    writeln!(
        out,
        "onboard plan (dry-run) for {} -> {}/{} on {}",
        plan.path, plan.owner, plan.repo, plan.host
    )
    .ok();
    for step in &plan.steps {
        writeln!(out, "  {}. {}: {}", step.order, step.action, step.detail).ok();
    }
    writeln!(
        out,
        "  (dry-run: server transport not live; no changes were made)"
    )
    .ok();
    Ok(())
}

/// Derive the repository name from the checkout path: the final non-empty path
/// component, with a trailing `.git` stripped.
fn repo_name(path: &str) -> ClientResult<String> {
    let trimmed = path.trim_end_matches('/');
    let last = trimmed
        .rsplit('/')
        .find(|c| !c.is_empty())
        .unwrap_or(trimmed);
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() || name == "." || name == ".." {
        return Err(ClientError::Invalid(format!(
            "cannot derive a repository name from path {path:?}"
        )));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_uses_final_component_and_strips_git() {
        assert_eq!(repo_name("/home/u/projects/alpha").unwrap(), "alpha");
        assert_eq!(repo_name("/home/u/projects/alpha/").unwrap(), "alpha");
        assert_eq!(repo_name("beta.git").unwrap(), "beta");
        assert_eq!(repo_name("gamma").unwrap(), "gamma");
    }

    #[test]
    fn repo_name_rejects_dot_paths() {
        assert!(repo_name(".").is_err());
        assert!(repo_name("..").is_err());
        assert!(repo_name("/").is_err());
    }
}
