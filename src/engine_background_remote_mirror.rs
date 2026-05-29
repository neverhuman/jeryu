//! Owner: Post-merge external-mirror consumer (Phase H)
//! Proof: `cargo test -p jeryu --lib engine_background_remote_mirror`
//! Invariants:
//!   - Runs ONLY on already-merged intents produced by
//!     `git::mirror_jobs` (the `gl_merge_mr` hook). It never pushes to GitHub
//!     before a local merge + CI success — it is a post-merge consumer, not a
//!     push-hook. (Contrast the anti-patterns in `release/full_path.rs` and
//!     `engine_webhook_push.rs` which act pre-merge — this module does NOT use
//!     them.)
//!   - An intent is marked CONSUMED only on a DEFINITIVE outcome (pushed, PR
//!     opened, PR already exists, or deliberately skipped). Transient failures
//!     stay unconsumed so the next loop retries — no lost mirrors, no retry
//!     storms (idempotent by `(owner,name,merged_sha)` key).
//!   - All side-effecting edges (repo resolution, git push, PR open) are
//!     behind traits so the decision logic is unit-tested without real git or
//!     network. Production wiring (`RealMirrorDeps`) composes
//!     `git::mirror::mirror_push` + `GitHubClient::create_pull_request` +
//!     `repo_fleet` registry lookup; the engine-spawn wiring is a follow-up.

use crate::git::mirror_jobs::{MirrorIntent, mirror_intents_log_path};
use crate::git_host::github::PullRequestOutcome;
use crate::policy_main_relay::{GithubRelayTarget, MainRelayPolicy};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Result of attempting to fast-forward push the merged ref to the external
/// `main`. The pusher classifies the git outcome so the consumer can decide
/// between "done", "fall back to a PR", and "retry later".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushAttempt {
    /// Push landed on the protected/unprotected branch — mirror complete.
    Pushed,
    /// Remote rejected the fast-forward (branch protected or non-ff). The
    /// consumer falls back to opening a PR when policy allows.
    Rejected,
    /// Transient failure (network, auth blip). Leave the intent unconsumed so
    /// the next loop retries.
    Retryable(String),
}

/// Terminal disposition of one mirror intent. Only the `*Done` / `Skipped*`
/// variants mark the intent consumed; `Deferred` keeps it for retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorOutcome {
    Pushed,
    PrOpened { url: Option<String> },
    PrAlreadyOpen,
    SkippedNoPolicy,
    SkippedNoLocalRepo,
    SkippedAlreadyConsumed,
    Deferred(String),
}

impl MirrorOutcome {
    /// Whether this outcome means the intent is fully handled and should be
    /// recorded as consumed (so it is never processed again).
    pub fn is_terminal(&self) -> bool {
        !matches!(self, MirrorOutcome::Deferred(_))
    }
}

/// Resolve a GitLab repo (owner/name) to its local working-tree path so the
/// consumer can `git push` the external remote from it.
pub trait RepoResolver {
    fn resolve(&self, owner: &str, name: &str) -> Option<PathBuf>;
}

/// Push the merged ref to the configured external remote's branch.
pub trait MainPusher {
    fn push_main(&self, repo_path: &Path, remote: &str, branch: &str) -> PushAttempt;
}

/// Open a PR `head -> base` on the external host (the protected-branch fallback).
pub trait PrOpener {
    fn open_pr(
        &self,
        owner: &str,
        name: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<PullRequestOutcome>;
}

/// Load the relay policy for a repo path. Behind a trait so tests can inject
/// a policy without a `.jeryu/policy.toml` on disk.
pub trait PolicyLoader {
    fn load(&self, repo_path: &Path) -> Result<MainRelayPolicy>;
}

/// Default policy loader: reads `<repo>/.jeryu/policy.toml`.
pub struct FilePolicyLoader;
impl PolicyLoader for FilePolicyLoader {
    fn load(&self, repo_path: &Path) -> Result<MainRelayPolicy> {
        MainRelayPolicy::for_repo(repo_path)
    }
}

/// The injected edges the consumer needs.
pub struct MirrorDeps<'a> {
    pub resolver: &'a dyn RepoResolver,
    pub policy: &'a dyn PolicyLoader,
    pub pusher: &'a dyn MainPusher,
    pub pr: &'a dyn PrOpener,
}

/// Process a single intent. Pure decision logic over injected edges — no I/O of
/// its own beyond what the traits perform.
pub fn process_intent(intent: &MirrorIntent, deps: &MirrorDeps<'_>) -> MirrorOutcome {
    let repo_path = match deps.resolver.resolve(&intent.repo_owner, &intent.repo_name) {
        Some(p) => p,
        None => return MirrorOutcome::SkippedNoLocalRepo,
    };
    let policy = match deps.policy.load(&repo_path) {
        Ok(p) => p,
        // A policy read failure is transient-ish (file races); defer.
        Err(e) => return MirrorOutcome::Deferred(format!("policy load: {e}")),
    };
    let target: GithubRelayTarget = match policy.github_relay_target() {
        Some(t) => t,
        None => return MirrorOutcome::SkippedNoPolicy,
    };

    match deps
        .pusher
        .push_main(&repo_path, &target.remote, &target.branch)
    {
        PushAttempt::Pushed => MirrorOutcome::Pushed,
        PushAttempt::Retryable(reason) => MirrorOutcome::Deferred(format!("push: {reason}")),
        PushAttempt::Rejected => {
            if !target.fallback_to_pr {
                return MirrorOutcome::Deferred(
                    "push rejected and fallback_to_pr disabled".to_string(),
                );
            }
            // Protected-branch fallback: open a PR from an ephemeral relay
            // branch into the external base. The relay branch name is
            // deterministic per merged SHA so re-runs hit GitHub's
            // "already exists" idempotency rather than spawning duplicates.
            let head = relay_branch_name(intent);
            let title = format!("jeryu relay: {} -> {}", short_sha(intent), target.branch);
            let body = relay_pr_body(intent, &target.branch);
            match deps.pr.open_pr(
                &intent.repo_owner,
                &intent.repo_name,
                &head,
                &target.branch,
                &title,
                &body,
            ) {
                Ok(PullRequestOutcome::Created { url, .. }) => MirrorOutcome::PrOpened { url },
                Ok(PullRequestOutcome::AlreadyExists) => MirrorOutcome::PrAlreadyOpen,
                Err(e) => MirrorOutcome::Deferred(format!("pr open: {e}")),
            }
        }
    }
}

fn short_sha(intent: &MirrorIntent) -> String {
    intent
        .merged_sha
        .as_deref()
        .map(|s| s.chars().take(10).collect())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Deterministic ephemeral relay branch name so retries are idempotent.
pub fn relay_branch_name(intent: &MirrorIntent) -> String {
    format!("jeryu-relay/{}", short_sha(intent))
}

fn relay_pr_body(intent: &MirrorIntent, base: &str) -> String {
    let url = intent.merge_url.as_deref().unwrap_or("(local merge)");
    format!(
        "Automated post-merge relay of `{}` into `{}`.\n\nSource merge: {}\n\nOpened because the external `{}` is protected (fast-forward push rejected). \
Merge or close after review.",
        short_sha(intent),
        base,
        url,
        base,
    )
}

/// Idempotency key for an intent.
pub fn consumed_key(intent: &MirrorIntent) -> String {
    format!(
        "{}/{}@{}",
        intent.repo_owner,
        intent.repo_name,
        intent.merged_sha.as_deref().unwrap_or("none")
    )
}

/// Append-only record of consumed intent keys at
/// `~/.jeryu/mirror_consumed.log` (one key per line).
pub fn consumed_log_path() -> PathBuf {
    mirror_intents_log_path().with_file_name("mirror_consumed.log")
}

fn read_consumed(path: &Path) -> BTreeSet<String> {
    match std::fs::read_to_string(path) {
        Ok(body) => body.lines().map(|l| l.trim().to_string()).collect(),
        Err(_) => BTreeSet::new(),
    }
}

fn append_consumed(path: &Path, key: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    writeln!(f, "{key}").with_context(|| format!("append {}", path.display()))
}

fn read_intents(path: &Path) -> Result<Vec<MirrorIntent>> {
    let body = match std::fs::read_to_string(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    let mut out = Vec::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        out.push(serde_json::from_str::<MirrorIntent>(line).context("parse mirror intent")?);
    }
    Ok(out)
}

/// One pass over the intent log: process each not-yet-consumed intent and
/// record terminal outcomes as consumed. Returns the per-intent outcomes for
/// logging/eventing. Pure-ish: file edges use the standard paths; decision
/// edges use `deps`.
pub fn run_once(
    intents_path: &Path,
    consumed_path: &Path,
    deps: &MirrorDeps<'_>,
) -> Result<Vec<(MirrorIntent, MirrorOutcome)>> {
    let intents = read_intents(intents_path)?;
    let consumed = read_consumed(consumed_path);
    let mut results = Vec::new();
    for intent in intents {
        let key = consumed_key(&intent);
        if consumed.contains(&key) {
            results.push((intent, MirrorOutcome::SkippedAlreadyConsumed));
            continue;
        }
        let outcome = process_intent(&intent, deps);
        if outcome.is_terminal() {
            append_consumed(consumed_path, &key)?;
        }
        results.push((intent, outcome));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    struct FakeResolver(HashMap<String, PathBuf>);
    impl RepoResolver for FakeResolver {
        fn resolve(&self, owner: &str, name: &str) -> Option<PathBuf> {
            self.0.get(&format!("{owner}/{name}")).cloned()
        }
    }

    struct FixedPolicy(MainRelayPolicy);
    impl PolicyLoader for FixedPolicy {
        fn load(&self, _repo: &Path) -> Result<MainRelayPolicy> {
            Ok(self.0.clone())
        }
    }

    struct ScriptedPusher(PushAttempt);
    impl MainPusher for ScriptedPusher {
        fn push_main(&self, _p: &Path, _r: &str, _b: &str) -> PushAttempt {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct RecordingPr {
        calls: RefCell<Vec<String>>,
        outcome: Option<PullRequestOutcome>,
    }
    impl PrOpener for RecordingPr {
        fn open_pr(
            &self,
            _o: &str,
            _n: &str,
            head: &str,
            base: &str,
            _t: &str,
            _b: &str,
        ) -> Result<PullRequestOutcome> {
            self.calls.borrow_mut().push(format!("{head}->{base}"));
            Ok(self.outcome.clone().unwrap_or(PullRequestOutcome::Created {
                number: "1".into(),
                url: Some("http://gh/pr/1".into()),
            }))
        }
    }

    fn intent(sha: &str) -> MirrorIntent {
        MirrorIntent {
            schema_version: 1,
            enqueued_at: chrono::Utc::now(),
            repo_owner: "acme".into(),
            repo_name: "widget".into(),
            merged_sha: Some(sha.into()),
            merge_url: Some("http://gl/mr/1".into()),
        }
    }

    fn policy_with_relay(fallback: bool) -> MainRelayPolicy {
        MainRelayPolicy::from_toml_str(&format!(
            "[main_relay]\nenabled = true\n[main_relay.github]\nenabled = true\nremote = \"github\"\nbranch = \"main\"\nfallback_to_pr = {fallback}\n"
        ))
        .unwrap()
    }

    fn resolver() -> FakeResolver {
        let mut m = HashMap::new();
        m.insert("acme/widget".to_string(), PathBuf::from("/repos/widget"));
        FakeResolver(m)
    }

    #[test]
    fn skips_when_no_local_repo() {
        let deps = MirrorDeps {
            resolver: &FakeResolver(HashMap::new()),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Pushed),
            pr: &RecordingPr::default(),
        };
        assert_eq!(
            process_intent(&intent("deadbeefcafe"), &deps),
            MirrorOutcome::SkippedNoLocalRepo
        );
    }

    #[test]
    fn skips_when_policy_off() {
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(MainRelayPolicy::default()),
            pusher: &ScriptedPusher(PushAttempt::Pushed),
            pr: &RecordingPr::default(),
        };
        assert_eq!(
            process_intent(&intent("deadbeefcafe"), &deps),
            MirrorOutcome::SkippedNoPolicy
        );
    }

    #[test]
    fn push_ok_is_terminal() {
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Pushed),
            pr: &RecordingPr::default(),
        };
        let o = process_intent(&intent("deadbeefcafe"), &deps);
        assert_eq!(o, MirrorOutcome::Pushed);
        assert!(o.is_terminal());
    }

    #[test]
    fn rejected_falls_back_to_pr() {
        let pr = RecordingPr::default();
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Rejected),
            pr: &pr,
        };
        let o = process_intent(&intent("deadbeefcafe"), &deps);
        assert_eq!(
            o,
            MirrorOutcome::PrOpened {
                url: Some("http://gh/pr/1".into())
            }
        );
        // deterministic relay branch -> base
        assert_eq!(
            pr.calls.borrow().as_slice(),
            &["jeryu-relay/deadbeefca->main"]
        );
    }

    #[test]
    fn rejected_without_fallback_defers() {
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(false)),
            pusher: &ScriptedPusher(PushAttempt::Rejected),
            pr: &RecordingPr::default(),
        };
        let o = process_intent(&intent("deadbeefcafe"), &deps);
        assert!(matches!(o, MirrorOutcome::Deferred(_)));
        assert!(!o.is_terminal(), "deferred must NOT be consumed");
    }

    #[test]
    fn pr_already_exists_is_terminal_idempotent() {
        let pr = RecordingPr {
            outcome: Some(PullRequestOutcome::AlreadyExists),
            ..Default::default()
        };
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Rejected),
            pr: &pr,
        };
        let o = process_intent(&intent("deadbeefcafe"), &deps);
        assert_eq!(o, MirrorOutcome::PrAlreadyOpen);
        assert!(o.is_terminal());
    }

    #[test]
    fn retryable_push_defers_not_consumed() {
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Retryable("network".into())),
            pr: &RecordingPr::default(),
        };
        let o = process_intent(&intent("deadbeefcafe"), &deps);
        assert!(matches!(o, MirrorOutcome::Deferred(_)));
        assert!(!o.is_terminal());
    }

    // Shared run_once fixture: a temp dir + the two log paths. The TempDir is
    // returned so the caller keeps it alive for the test's duration.
    fn log_paths() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::TempDir::new().unwrap();
        let intents = tmp.path().join("mirror_intents.jsonl");
        let consumed = tmp.path().join("mirror_consumed.log");
        (tmp, intents, consumed)
    }

    #[test]
    fn run_once_consumes_terminal_and_retries_deferred() {
        let (_tmp, intents_path, consumed_path) = log_paths();
        // one intent that pushes ok
        std::fs::write(
            &intents_path,
            format!(
                "{}\n",
                serde_json::to_string(&intent("aaaa111122")).unwrap()
            ),
        )
        .unwrap();

        let ok_deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Pushed),
            pr: &RecordingPr::default(),
        };
        let r1 = run_once(&intents_path, &consumed_path, &ok_deps).unwrap();
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].1, MirrorOutcome::Pushed);
        // second pass: already consumed -> skipped, push NOT attempted again
        let r2 = run_once(&intents_path, &consumed_path, &ok_deps).unwrap();
        assert_eq!(r2[0].1, MirrorOutcome::SkippedAlreadyConsumed);
    }

    #[test]
    fn run_once_does_not_consume_deferred() {
        let (_tmp, intents_path, consumed_path) = log_paths();
        std::fs::write(
            &intents_path,
            format!(
                "{}\n",
                serde_json::to_string(&intent("bbbb222233")).unwrap()
            ),
        )
        .unwrap();
        let defer_deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Retryable("blip".into())),
            pr: &RecordingPr::default(),
        };
        let r1 = run_once(&intents_path, &consumed_path, &defer_deps).unwrap();
        assert!(matches!(r1[0].1, MirrorOutcome::Deferred(_)));
        // not consumed -> next pass tries again (still deferred), never skipped
        let r2 = run_once(&intents_path, &consumed_path, &defer_deps).unwrap();
        assert!(matches!(r2[0].1, MirrorOutcome::Deferred(_)));
    }

    #[test]
    fn run_once_missing_intents_file_is_empty() {
        // intents file is never written → run_once treats it as empty.
        let (_tmp, intents_path, consumed_path) = log_paths();
        let deps = MirrorDeps {
            resolver: &resolver(),
            policy: &FixedPolicy(policy_with_relay(true)),
            pusher: &ScriptedPusher(PushAttempt::Pushed),
            pr: &RecordingPr::default(),
        };
        let r = run_once(&intents_path, &consumed_path, &deps).unwrap();
        assert!(r.is_empty());
    }
}
