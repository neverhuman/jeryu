//! Direct merge-to-GitHub main mirroring.
//!
//! When a PR merges into a repo's default branch on the local forge, the merge
//! handler pushes the LIVE branch tip straight to the configured GitHub
//! repository (`github.com/<github_slug>` main). Targets come from the split
//! manifests: a `[[repo]]` entry participates iff it carries `github_slug`,
//! `jeryu_slug`, and `mirror_github_main = true`.
//!
//! Auth rides the host's proven shim: the destination URL is built as
//! `https://x-access-token:jeryussh@github.com/<slug>.git`, which the global
//! gitconfig `url."git@github-mirror:".insteadOf` rewrite turns into an SSH
//! push with the neverhuman deploy key. Nothing secret-looking is stored in
//! the manifest (the relay token is a fixed public dummy).
//!
//! Failure isolation: a GitHub push failure NEVER fails the merge. The outcome
//! is recorded as a `jeryu/github-mirror` check-run on the merged tip so the
//! posture is visible next to CI, matching the legacy external relay's
//! convention.

#![cfg(feature = "web")]

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Hard wall-clock bound for one push; a hung network push must never wedge
/// the merge handler (the push runs synchronously inside it).
const PUSH_TIMEOUT: Duration = Duration::from_secs(60);

/// Check-run name used to record push outcomes (legacy relay convention).
pub const MIRROR_CHECK_NAME: &str = "jeryu/github-mirror";

#[derive(Clone, Debug, Default)]
pub struct GithubMirror {
    enabled: bool,
    /// Keyed by lowercased local slug `owner/name` (the manifest `jeryu_slug`).
    targets: BTreeMap<String, GithubMirrorTarget>,
}

#[derive(Clone, Debug)]
pub struct GithubMirrorTarget {
    pub github_slug: String,
    pub branch: String,
    /// Test seam: a local path or URL that replaces the GitHub destination.
    pub destination_override: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirrorPushOutcome {
    Pushed { tip: String },
    Skipped(String),
    Failed(String),
}

#[derive(Debug, serde::Deserialize)]
struct MirrorManifest {
    repo: Option<Vec<MirrorManifestRepo>>,
}

#[derive(Debug, serde::Deserialize)]
struct MirrorManifestRepo {
    github_slug: Option<String>,
    jeryu_slug: Option<String>,
    default_branch: Option<String>,
    #[serde(default)]
    mirror_github_main: bool,
}

impl GithubMirror {
    /// Load targets from split manifests. Absent/unparsable manifests, or
    /// `JERYU_GITHUB_PUSH=0`, yields a disabled mirror (every push skips).
    pub fn load(manifests: &[PathBuf]) -> Self {
        if std::env::var("JERYU_GITHUB_PUSH").as_deref() == Ok("0") {
            return Self::default();
        }
        Self::from_manifests(manifests)
    }

    fn from_manifests(manifests: &[PathBuf]) -> Self {
        if manifests.is_empty() {
            return Self::default();
        }
        let mut targets = BTreeMap::new();
        for path in manifests {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<MirrorManifest>(&text) else {
                continue;
            };
            for repo in parsed.repo.unwrap_or_default() {
                let (Some(github_slug), Some(jeryu_slug)) = (repo.github_slug, repo.jeryu_slug)
                else {
                    continue;
                };
                if !repo.mirror_github_main {
                    continue;
                }
                targets.insert(
                    jeryu_slug.to_ascii_lowercase(),
                    GithubMirrorTarget {
                        github_slug,
                        branch: repo.default_branch.unwrap_or_else(|| "main".to_string()),
                        destination_override: None,
                    },
                );
            }
        }
        Self {
            enabled: !targets.is_empty(),
            targets,
        }
    }

    /// Embedding/test seam: build a mirror from explicit targets (e.g. a local
    /// bare destination via `destination_override`).
    pub fn with_targets(targets: BTreeMap<String, GithubMirrorTarget>) -> Self {
        Self {
            enabled: !targets.is_empty(),
            targets,
        }
    }

    pub fn target(&self, owner: &str, name: &str) -> Option<&GithubMirrorTarget> {
        if !self.enabled {
            return None;
        }
        self.targets
            .get(&format!("{}/{}", owner, name).to_ascii_lowercase())
    }

    /// Push the LIVE tip of the target branch in `bare` to the GitHub remote.
    ///
    /// Resolves the branch tip at call time (NOT the merge oid) because the
    /// post-push bridge may have appended a `chore(release)` autoversion commit
    /// after the merge moved the ref. Fast-forward only: no `--force`.
    pub fn push_branch(
        &self,
        git_bin: &str,
        bare: &Path,
        owner: &str,
        name: &str,
    ) -> MirrorPushOutcome {
        let Some(target) = self.target(owner, name) else {
            return MirrorPushOutcome::Skipped(format!(
                "{owner}/{name} is not a github-mirror target"
            ));
        };
        let tip = match run_bounded(
            git_bin,
            &["rev-parse", &format!("refs/heads/{}", target.branch)],
            bare,
        ) {
            Ok(out) => out.trim().to_string(),
            Err(err) => return MirrorPushOutcome::Failed(format!("resolve tip: {err}")),
        };
        if tip.is_empty() {
            return MirrorPushOutcome::Failed(format!(
                "refs/heads/{} did not resolve in {}",
                target.branch,
                bare.display()
            ));
        }
        let dest = target.destination_override.clone().unwrap_or_else(|| {
            format!(
                "https://x-access-token:jeryussh@github.com/{}.git",
                target.github_slug
            )
        });
        let refspec = format!("{}:refs/heads/{}", tip, target.branch);
        match run_bounded(git_bin, &["push", &dest, &refspec], bare) {
            Ok(_) => MirrorPushOutcome::Pushed { tip },
            Err(err) => MirrorPushOutcome::Failed(redact(&err)),
        }
    }
}

/// Run git with prompts disabled and a hard timeout; returns stdout on success.
fn run_bounded(git_bin: &str, args: &[&str], cwd: &Path) -> Result<String, String> {
    let mut child = std::process::Command::new(git_bin)
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o ConnectTimeout=10",
        )
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| format!("spawn {git_bin}: {err}"))?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_string(&mut stdout);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut stderr);
                }
                if status.success() {
                    return Ok(stdout);
                }
                return Err(format!("git {} failed: {}", args.join(" "), stderr.trim()));
            }
            Ok(None) => {
                if start.elapsed() > PUSH_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "git {} timed out after {}s",
                        args.join(" "),
                        PUSH_TIMEOUT.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(format!("wait: {err}")),
        }
    }
}

/// Strip the relay-token userinfo from any URL echoed in git errors.
///
/// Single forward pass: everything between `x-access-token:` and the next `@`
/// becomes `***`, and scanning resumes AFTER the rewritten span so the
/// replacement can never re-match itself.
fn redact(text: &str) -> String {
    const MARK: &str = "x-access-token:";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(MARK) {
        let after = start + MARK.len();
        out.push_str(&rest[..after]);
        let tail = &rest[after..];
        match tail.find('@') {
            Some(at) => {
                out.push_str("***");
                rest = &tail[at..];
            }
            None => {
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_switch_disables_targets() {
        // Pure predicate check (mirrors ci_bridge::mock_flag_set style): the
        // env-driven branch is exercised via load() in integration tests; here
        // we prove an empty target set always skips.
        let mirror = GithubMirror::default();
        assert!(mirror.target("jeryu", "jeryu-core").is_none());
    }

    #[test]
    fn manifest_targets_match_jeryu_slug_case_insensitively() {
        let mut targets = BTreeMap::new();
        targets.insert(
            "jeryu/jeryu-core".to_string(),
            GithubMirrorTarget {
                github_slug: "neverhuman/jeryu-core".to_string(),
                branch: "main".to_string(),
                destination_override: None,
            },
        );
        let mirror = GithubMirror::with_targets(targets);
        assert!(mirror.target("Jeryu", "JERYU-CORE").is_some());
        assert!(mirror.target("jeryu", "other").is_none());
    }

    #[test]
    fn manifest_targets_aggregate_from_multiple_manifests() {
        let root = tempfile::tempdir().expect("manifest dir");
        let jeryu_manifest = root.path().join("jeryu.toml");
        let jekko_manifest = root.path().join("jekko.toml");
        std::fs::write(
            &jeryu_manifest,
            r#"
[[repo]]
github_slug = "neverhuman/jeryu-core"
jeryu_slug = "jeryu/jeryu-core"
default_branch = "main"
mirror_github_main = true
"#,
        )
        .expect("write jeryu manifest");
        std::fs::write(
            &jekko_manifest,
            r#"
[[repo]]
github_slug = "neverhuman/jekko-core"
jeryu_slug = "jeryu/jekko-core"
default_branch = "main"
mirror_github_main = true

[[repo]]
github_slug = "neverhuman/jekko-scratch"
jeryu_slug = "jeryu/jekko-scratch"
default_branch = "main"
mirror_github_main = false
"#,
        )
        .expect("write jekko manifest");

        let mirror = GithubMirror::from_manifests(&[jeryu_manifest, jekko_manifest]);
        assert_eq!(
            mirror
                .target("jeryu", "jeryu-core")
                .map(|target| target.github_slug.as_str()),
            Some("neverhuman/jeryu-core")
        );
        assert_eq!(
            mirror
                .target("jeryu", "jekko-core")
                .map(|target| target.github_slug.as_str()),
            Some("neverhuman/jekko-core")
        );
        assert!(
            mirror.target("jeryu", "jekko-scratch").is_none(),
            "mirror_github_main=false entries are ignored"
        );
    }

    #[test]
    fn redact_strips_relay_token() {
        let raw = "fatal: unable to access 'https://x-access-token:jeryussh@github.com/x.git'";
        let cleaned = redact(raw);
        assert!(cleaned.contains("x-access-token:***@github.com"));
        assert!(!cleaned.contains("jeryussh"));
    }
}
