//! Owner: WS2 repo hygiene audit — detect git/auth footguns + confused agent files.
//! Proof: `cargo test -p jeryu --lib repo_hygiene_audit`
//! Invariants:
//!   - READ-ONLY. Returns findings; never mutates a repo, remote, credential, or
//!     file. This is the "smart audit" half of WS2: surface the footguns the
//!     jeryu standard forbids (HTTP-PAT origins, glab/credential bypass, stale
//!     agent files, missing `.jeryu/`) so an agent/operator gets clear feedback.
//!   - Self-contained: the footgun classifiers are pure string checks so they
//!     unit-test without a repo, network, or the (still-unmerged) canonical
//!     standard.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "ERROR",
            Severity::Warning => "WARN",
            Severity::Info => "INFO",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneFinding {
    pub kind: &'static str,
    pub severity: Severity,
    pub detail: String,
    pub fix_hint: &'static str,
}

/// Is an origin URL the forbidden HTTP local-GitLab footgun (vs. canonical SSH)?
pub fn is_http_local_gitlab(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://")
        && (u.contains("localhost:8929")
            || u.contains("gitlab.local")
            || u.contains("127.0.0.1:8929"))
}

/// Does an `AGENTS.md` look like a *confused/stale fork* — it talks about local
/// GitLab access but never mandates SSH remotes (the exact jekko failure mode)?
/// Conservative: only flags when access is discussed AND SSH is never mentioned.
pub fn agents_md_is_confused(contents: &str) -> bool {
    let lc = contents.to_ascii_lowercase();
    let talks_about_access = lc.contains("local gitlab")
        || lc.contains("access contract")
        || lc.contains("remote origin");
    let mentions_ssh = lc.contains("ssh remote") || lc.contains("ssh://");
    talks_about_access && !mentions_ssh
}

/// Best-effort read of a repo's `origin` URL (no network).
fn read_origin(repo: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Audit one repo for git/auth + agent-file hygiene footguns. READ-ONLY.
pub fn audit_repo(repo: &Path) -> Vec<HygieneFinding> {
    let mut findings = Vec::new();

    match read_origin(repo) {
        Some(url) if is_http_local_gitlab(&url) => findings.push(HygieneFinding {
            kind: "http_pat_origin",
            severity: Severity::Error,
            detail: format!(
                "origin is an HTTP local-GitLab URL ({url}); the standard requires keyless SSH"
            ),
            fix_hint:
                "git remote set-url origin ssh://git@127.0.0.1:2224/<ns>/<repo>.git (or `jeryu access repair`)",
        }),
        None => findings.push(HygieneFinding {
            kind: "missing_origin",
            severity: Severity::Warning,
            detail: "no git origin configured".into(),
            fix_hint: "add the canonical SSH origin",
        }),
        _ => {}
    }

    let agents = repo.join("AGENTS.md");
    match std::fs::read_to_string(&agents) {
        Ok(contents) if agents_md_is_confused(&contents) => findings.push(HygieneFinding {
            kind: "confused_agent_file",
            severity: Severity::Warning,
            detail: "AGENTS.md describes local-GitLab access but never mandates SSH remotes (stale fork)"
                .into(),
            fix_hint: "re-sync the access-contract clause to require 'local GitLab SSH remotes'",
        }),
        Ok(_) => {}
        Err(_) => findings.push(HygieneFinding {
            kind: "missing_agents_md",
            severity: Severity::Info,
            detail: "no AGENTS.md".into(),
            fix_hint: "add the standard AGENTS.md from the repo-standard template",
        }),
    }

    if !repo.join(".jeryu").is_dir() {
        findings.push(HygieneFinding {
            kind: "missing_jeryu_dir",
            severity: Severity::Info,
            detail: "no .jeryu/ directory".into(),
            fix_hint: "run `jeryu repo standard apply`",
        });
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_http_local_gitlab_origin() {
        assert!(is_http_local_gitlab("http://localhost:8929/root/jekko.git"));
        assert!(is_http_local_gitlab("http://gitlab.local:8929/root/x.git"));
        assert!(is_http_local_gitlab("http://127.0.0.1:8929/root/x.git"));
        // canonical SSH is fine
        assert!(!is_http_local_gitlab("ssh://git@127.0.0.1:2224/root/jekko.git"));
        // external https github is not the local footgun
        assert!(!is_http_local_gitlab("https://github.com/org/repo.git"));
    }

    #[test]
    fn flags_stale_fork_agents_md_missing_ssh_mandate() {
        // jekko's exact stale fork: access contract, glab, http origins — no SSH.
        let stale = "Access contract: local agent workspaces use ~/.jeryu/access.toml; \
                     do not install glab or keep HTTP local GitLab origins.";
        assert!(agents_md_is_confused(stale));

        // canonical: mentions SSH remotes → not confused.
        let canonical = "Access contract: local agent workspaces use local GitLab SSH remotes \
                         (ssh://git@127.0.0.1:2224/root/<repo>.git).";
        assert!(!agents_md_is_confused(canonical));

        // an AGENTS.md that doesn't talk about access at all → not flagged.
        assert!(!agents_md_is_confused("Read AGENTS.md first. Use the jankurai standard."));
    }
}
