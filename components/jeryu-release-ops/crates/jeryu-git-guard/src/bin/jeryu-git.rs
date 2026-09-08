//! `jeryu-git` — the confined-agent git wrapper.
//!
//! Installed as the ONLY `git` on a sandboxed agent's `PATH`. It consults the pure
//! [`jeryu_git_guard::git_command_decision`] verdict and then either `exec`s the real
//! git (replacing this process, so no overhead) or prints typed repair guidance and
//! exits non-zero. The agent's assigned branch comes from `JERYU_BRANCH`; the real
//! git binary path comes from `JERYU_REAL_GIT` (default `/usr/bin/git`).

use std::os::unix::process::CommandExt;
use std::process::Command;

use jeryu_git_guard::{DENY_EXIT_CODE, GitDecision, git_command_decision, leading_subcommand};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let assigned_branch = match std::env::var("JERYU_BRANCH") {
        Ok(branch) if !branch.trim().is_empty() => branch,
        _ => {
            eprintln!("jeryu-git: missing required JERYU_BRANCH");
            std::process::exit(2);
        }
    };

    match git_command_decision(&argv, &assigned_branch) {
        GitDecision::Allow => {
            // A `commit` that passed the verdict still has to clear the jankurai
            // audit gate: any newly-introduced cap / hard finding blocks it here,
            // before the commit lands, and the agent must resolve it. The gate
            // runs in THIS wrapper (the only `git` on the agent PATH), so it
            // cannot be skipped — `--no-verify`/`-n` are already refused.
            if leading_subcommand(&argv) == Some("commit") {
                run_commit_gate();
            }
            let real_git = match std::env::var("JERYU_REAL_GIT") {
                Ok(path) if !path.trim().is_empty() => path,
                _ => {
                    eprintln!("jeryu-git: missing required JERYU_REAL_GIT");
                    std::process::exit(2);
                }
            };
            // `exec` replaces this process; it only returns on failure.
            let err = Command::new(&real_git).args(&argv).exec();
            eprintln!("jeryu-git: failed to exec real git ({real_git}): {err}");
            std::process::exit(127);
        }
        GitDecision::Deny(error) => {
            eprintln!("{}", error.render());
            std::process::exit(DENY_EXIT_CODE);
        }
    }
}

/// Run the pinned jankurai diff-audit over the about-to-be-committed change set.
/// Returns normally only when the audit passes; on any failure it prints typed
/// repair guidance and exits, so the real `git commit` never runs.
///
/// Fail-closed: a missing auditor binary refuses the commit rather than waving it
/// through. `JANKURAI_SKIP_HOOKS` is forced off so the agent cannot disable the
/// gate via the env. Diff-only (`diff-audit`), so it is fast.
fn run_commit_gate() {
    let jankurai = match std::env::var("JERYU_JANKURAI_BIN") {
        Ok(bin) if !bin.trim().is_empty() => bin,
        _ => {
            eprintln!(
                "jeryu-git: refused `git commit` — git_commit_gate_unavailable\n  \
                 why: the jankurai audit gate must run on every commit, but \
                 JERYU_JANKURAI_BIN is unset\n  hint: this is a sandbox \
                 misconfiguration; the auditor is baked into the agent image"
            );
            std::process::exit(DENY_EXIT_CODE);
        }
    };
    // Base ref: the branch's local cut point, seeded by the runner
    // (JANKURAI_DIFF_BASE). Fall back to HEAD so the staged set is still scored
    // if the runner did not seed it.
    let base = std::env::var("JANKURAI_DIFF_BASE")
        .ok()
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| "HEAD".to_string());

    let status = Command::new(&jankurai)
        .arg("diff-audit")
        .arg(".")
        .arg("--base-ref")
        .arg(&base)
        .arg("--skip-proof")
        // The gate's job is to block NEW caps/hard findings: diff-audit exits
        // non-zero on exactly that (no --advisory-only).
        .env_remove("JANKURAI_SKIP_HOOKS")
        .env("JANKURAI_SKIP_HOOKS", "0")
        .status();

    match status {
        Ok(s) if s.success() => {} // gate passed — fall through to real git commit
        Ok(_) => {
            eprintln!(
                "jeryu-git: refused `git commit` — git_commit_audit_failed\n  \
                 why: this change introduces jankurai caps/hard findings (see the \
                 diff-audit output above and target/jankurai/diff/diff-score.md)\n  \
                 fixes:\n    - resolve the flagged findings, then commit again\n    \
                 - run `jankurai diff-audit . --base-ref {base}` to re-check\n  \
                 hint: the audit gate blocks new caps before they enter the system"
            );
            std::process::exit(DENY_EXIT_CODE);
        }
        Err(err) => {
            eprintln!(
                "jeryu-git: refused `git commit` — git_commit_gate_unavailable\n  \
                 why: could not run the jankurai audit gate ({jankurai}): {err}\n  \
                 hint: fail-closed; the auditor must be runnable for a commit to land"
            );
            std::process::exit(DENY_EXIT_CODE);
        }
    }
}
