//! Owner: Interactive TUI subsystem — live merged delivery collector
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::live_delivery`
//! Invariants:
//!   - The collector is read-only.
//!   - Transient failures preserve the last good snapshot when possible.
//!   - GitHub and GitLab PR/MR items are merged by repo + IID + head SHA.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use chrono::{Duration as ChronoDuration, Utc};
use sha2::{Digest, Sha256};

use crate::{
    git_host::{GitHost, PrSummary, RepoRef},
    release::ReleaseAttemptView,
    repo_fleet::{RepoConfig, RepoRegistry, load_registry_from},
    tui::{
        app::DeliverySourceStatus,
        workflow::{
            delivery::{DeploymentProgress, PrInput, TestSpec, collect_delivery_snapshot},
            model::{DeliverySnapshot, WorkflowStatus},
        },
    },
};

#[derive(Clone, Default)]
pub struct DeliveryHosts {
    pub github: Option<Arc<dyn GitHost>>,
    pub gitlab: Option<Arc<dyn GitHost>>,
}

impl DeliveryHosts {
    pub fn from_env() -> Self {
        use crate::git_host::{GitHubClient, GitLabClient};
        use crate::llm::secrets::{SecretResolver, resolve_secret};

        let resolver = SecretResolver::from_env();
        let github = resolve_secret("GITHUB_TOKEN", &resolver)
            .map(|secret| Arc::new(GitHubClient::new(secret.value)) as Arc<dyn GitHost>);
        let gitlab = GitLabClient::from_env()
            .ok()
            .map(|client| Arc::new(client) as Arc<dyn GitHost>);

        Self { github, gitlab }
    }

    pub fn backend_label(&self) -> Option<String> {
        match (self.github.is_some(), self.gitlab.is_some()) {
            (false, false) => None,
            (true, false) => Some("GitHub".into()),
            (false, true) => Some("GitLab".into()),
            (true, true) => Some("GitHub + GitLab".into()),
        }
    }

    fn host_for(&self, provider: &str) -> Option<&dyn GitHost> {
        match provider {
            "github" => self.github.as_deref(),
            "gitlab" => self.gitlab.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveDeliveryUpdate {
    pub snapshot: DeliverySnapshot,
    pub source_status: DeliverySourceStatus,
}

#[derive(Debug, Default)]
pub struct LiveDeliveryCollector {
    last_good_snapshot: Option<DeliverySnapshot>,
}

impl LiveDeliveryCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn sync(
        &mut self,
        workspace_root: Option<&Path>,
        hosts: &DeliveryHosts,
        release: Option<&ReleaseAttemptView>,
    ) -> LiveDeliveryUpdate {
        let now = chrono::Utc::now();
        let mut source_status = DeliverySourceStatus {
            configured: false,
            backend_label: hosts.backend_label(),
            source_label: None,
            last_sync_at: Some(now),
            last_sync_error: None,
        };

        let Some(root) = workspace_root else {
            return self.empty_update(source_status);
        };

        let registry = match load_registry_from(root) {
            Ok(registry) => registry,
            Err(err) => {
                source_status.source_label = Some(format!(
                    "fleet registry: {}",
                    root.join(crate::repo_fleet::DEFAULT_REGISTRY_PATH)
                        .display()
                ));
                source_status.last_sync_error = Some(err.to_string());
                return self.empty_update(source_status);
            }
        };

        source_status.configured = !registry.repo.is_empty()
            && registry
                .repo
                .iter()
                .any(|repo| hosts.host_for(&repo.provider).is_some());
        source_status.source_label = Some(format!(
            "fleet registry: {}",
            root.join(crate::repo_fleet::DEFAULT_REGISTRY_PATH)
                .display()
        ));

        let collected = self.collect_from_registry(&registry, hosts, release).await;
        let mut snapshot = collected.snapshot;
        if collected.had_error {
            source_status.last_sync_error = collected.error;
            snapshot.outdated = true;
            if let Some(last_good) = self.last_good_snapshot.clone() {
                let mut preserved = last_good;
                preserved.outdated = true;
                return LiveDeliveryUpdate {
                    snapshot: preserved,
                    source_status,
                };
            }
        } else {
            self.last_good_snapshot = Some(snapshot.clone());
        }

        if snapshot.pull_requests.is_empty() && source_status.configured {
            snapshot.outdated = collected.had_error;
        }

        LiveDeliveryUpdate {
            snapshot,
            source_status,
        }
    }

    fn empty_update(&self, source_status: DeliverySourceStatus) -> LiveDeliveryUpdate {
        let mut snapshot = self
            .last_good_snapshot
            .clone()
            .unwrap_or_else(DeliverySnapshot::empty);
        if self.last_good_snapshot.is_some() || source_status.last_sync_error.is_some() {
            snapshot.outdated = true;
        }
        LiveDeliveryUpdate {
            snapshot,
            source_status,
        }
    }

    async fn collect_from_registry(
        &self,
        registry: &RepoRegistry,
        hosts: &DeliveryHosts,
        release: Option<&ReleaseAttemptView>,
    ) -> LiveDeliveryCollectResult {
        let mut items: BTreeMap<DeliveryItemKey, LiveDeliveryItem> = BTreeMap::new();
        let mut failures: Vec<String> = Vec::new();
        let now = Utc::now();

        for repo in &registry.repo {
            let Some(host) = hosts.host_for(&repo.provider) else {
                failures.push(format!(
                    "{} backend is not configured for {}",
                    repo.provider, repo.alias
                ));
                continue;
            };

            let repo_ref = match RepoRef::parse(&repo.slug) {
                Some(repo_ref) => repo_ref,
                None => {
                    failures.push(format!("invalid repo slug for {}", repo.alias));
                    continue;
                }
            };

            let source_rank = match repo.provider.as_str() {
                "gitlab" => 2,
                _ => 1,
            };

            match host.list_open_prs(&repo_ref).await {
                Ok(prs) => {
                    for summary in prs {
                        let key = DeliveryItemKey::new(&repo.slug, &summary);
                        let item = build_live_item(repo, &summary, source_rank, now);
                        match items.get(&key) {
                            Some(existing) if existing.source_rank > source_rank => {}
                            _ => {
                                items.insert(key, item);
                            }
                        }
                    }
                }
                Err(err) => {
                    failures.push(format!("{} {}: {}", repo.provider, repo.slug, err));
                }
            }
        }

        let mut pr_inputs: Vec<PrInput> = items.into_values().map(|item| item.pr_input).collect();
        pr_inputs.sort_by(|a, b| {
            a.repo_alias
                .cmp(&b.repo_alias)
                .then_with(|| a.repo_slug.cmp(&b.repo_slug))
                .then_with(|| a.number.cmp(&b.number))
        });

        let snapshot = if pr_inputs.is_empty() {
            DeliverySnapshot::empty()
        } else {
            collect_delivery_snapshot(&pr_inputs, release)
        };

        LiveDeliveryCollectResult {
            snapshot,
            had_error: !failures.is_empty(),
            error: if failures.is_empty() {
                None
            } else {
                Some(failures.join("; "))
            },
        }
    }
}

struct LiveDeliveryCollectResult {
    snapshot: DeliverySnapshot,
    had_error: bool,
    error: Option<String>,
}

struct LiveDeliveryItem {
    source_rank: u8,
    pr_input: PrInput,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeliveryItemKey {
    repo_slug: String,
    mr_iid: String,
    head_sha: String,
}

impl DeliveryItemKey {
    fn new(repo_slug: &str, summary: &PrSummary) -> Self {
        Self {
            repo_slug: repo_slug.to_string(),
            mr_iid: summary.mr_iid.clone(),
            head_sha: summary.head_sha.clone(),
        }
    }
}

fn build_live_item(
    repo: &RepoConfig,
    summary: &PrSummary,
    source_rank: u8,
    now: chrono::DateTime<Utc>,
) -> LiveDeliveryItem {
    let number = parse_pr_number(&repo.slug, &summary.mr_iid);
    let created_at = now - ChronoDuration::seconds(1);
    let pr_input = PrInput {
        number,
        title: summary.title.clone(),
        author: summary.author.clone(),
        head_sha: summary.head_sha.clone(),
        created_at,
        draft: summary.draft,
        labels: summary.labels.clone(),
        pre_merge_tests: vec![TestSpec {
            id: "open".into(),
            label: format!("open PR · {}", repo.alias),
            command: format!("view {}", summary.mr_iid),
            status: WorkflowStatus::Waiting,
            progress_pct: None,
            eta_secs: None,
            duration_secs: None,
            reason: Some(format!("{} → {}", repo.alias, summary.target_branch)),
            critical_path: true,
        }],
        merged_into_main: false,
        post_merge_tests: Vec::new(),
        deployment: DeploymentProgress::default(),
        repo_alias: Some(repo.alias.clone()),
        repo_slug: Some(repo.slug.clone()),
    };

    LiveDeliveryItem {
        source_rank,
        pr_input,
    }
}

fn parse_pr_number(repo_slug: &str, mr_iid: &str) -> u64 {
    match mr_iid.parse() {
        Ok(number) => number,
        Err(_) => {
            let digest = Sha256::digest(format!("{repo_slug}:{mr_iid}").as_bytes());
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&digest[..8]);
            u64::from_be_bytes(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_host::test_utils::FakeGitHost;
    fn registry(root: &Path, entries: &[(&str, &str, &str)]) {
        let mut text = String::from("schema_version = \"1\"\n");
        for (alias, slug, provider) in entries {
            text.push_str("\n[[repo]]\n");
            text.push_str(&format!("alias = \"{}\"\n", alias));
            text.push_str(&format!("slug = \"{}\"\n", slug));
            text.push_str(&format!("provider = \"{}\"\n", provider));
            text.push_str(&format!(
                "remote = \"https://example.invalid/{}.git\"\n",
                alias
            ));
            text.push_str(&format!("local_root = \"{}\"\n", root.display()));
            text.push_str("default_branch = \"main\"\n");
            text.push_str("visibility = \"private\"\n");
            text.push_str("health_profile = \"split-required\"\n");
        }
        std::fs::create_dir_all(root.join(".jeryu")).unwrap();
        std::fs::write(root.join(".jeryu/repos.toml"), text).unwrap();
    }

    fn summary(iid: &str, head: &str, title: &str) -> PrSummary {
        PrSummary {
            mr_iid: iid.into(),
            head_sha: head.into(),
            target_branch: "main".into(),
            author: "octocat".into(),
            title: title.into(),
            draft: false,
            labels: vec![],
        }
    }

    #[tokio::test]
    async fn github_only_collects_prs() {
        let tmp = tempfile::TempDir::new().unwrap();
        registry(tmp.path(), &[("shared", "octo/widget", "github")]);
        let github = Arc::new(
            FakeGitHost::new()
                .with_open_prs("octo/widget", vec![summary("17", "sha17", "GitHub PR")]),
        );
        let hosts = DeliveryHosts {
            github: Some(github),
            gitlab: None,
        };
        let mut collector = LiveDeliveryCollector::new();
        let update = collector.sync(Some(tmp.path()), &hosts, None).await;
        assert!(update.source_status.configured);
        assert_eq!(update.snapshot.pull_requests.len(), 1);
        assert_eq!(update.snapshot.pull_requests[0].number, 17);
    }

    #[tokio::test]
    async fn gitlab_only_collects_prs() {
        let tmp = tempfile::TempDir::new().unwrap();
        registry(tmp.path(), &[("deploy", "root/veox-deploy", "gitlab")]);
        let gitlab = Arc::new(FakeGitHost::new().with_open_prs(
            "root/veox-deploy",
            vec![summary("42", "sha42", "GitLab MR")],
        ));
        let hosts = DeliveryHosts {
            github: None,
            gitlab: Some(gitlab),
        };
        let mut collector = LiveDeliveryCollector::new();
        let update = collector.sync(Some(tmp.path()), &hosts, None).await;
        assert!(update.source_status.configured);
        assert_eq!(update.snapshot.pull_requests.len(), 1);
        assert_eq!(update.snapshot.pull_requests[0].title, "GitLab MR");
    }

    #[tokio::test]
    async fn merged_items_dedupe_by_repo_and_head() {
        let tmp = tempfile::TempDir::new().unwrap();
        registry(tmp.path(), &[("shared", "octo/widget", "github")]);
        let summary = summary("17", "sha17", "Merged PR");
        let github =
            Arc::new(FakeGitHost::new().with_open_prs("octo/widget", vec![summary.clone()]));
        let gitlab = Arc::new(FakeGitHost::new().with_open_prs("octo/widget", vec![summary]));
        let hosts = DeliveryHosts {
            github: Some(github),
            gitlab: Some(gitlab),
        };
        let mut collector = LiveDeliveryCollector::new();
        let update = collector.sync(Some(tmp.path()), &hosts, None).await;
        assert_eq!(update.snapshot.pull_requests.len(), 1);
        assert_eq!(
            update.source_status.backend_label.as_deref(),
            Some("GitHub + GitLab")
        );
    }

    #[tokio::test]
    async fn missing_registry_returns_empty_snapshot() {
        let hosts = DeliveryHosts::default();
        let mut collector = LiveDeliveryCollector::new();
        let update = collector.sync(None, &hosts, None).await;
        assert!(!update.source_status.configured);
        assert!(update.snapshot.pull_requests.is_empty());
    }

    #[tokio::test]
    async fn transient_failure_preserves_last_good_snapshot() {
        let tmp = tempfile::TempDir::new().unwrap();
        registry(tmp.path(), &[("shared", "octo/widget", "github")]);
        let github = Arc::new(
            FakeGitHost::new()
                .with_open_prs("octo/widget", vec![summary("17", "sha17", "First PR")]),
        );
        let hosts = DeliveryHosts {
            github: Some(github.clone()),
            gitlab: None,
        };
        let mut collector = LiveDeliveryCollector::new();
        let first = collector.sync(Some(tmp.path()), &hosts, None).await;
        assert_eq!(first.snapshot.pull_requests.len(), 1);

        let failing_hosts = DeliveryHosts {
            github: Some(Arc::new(
                FakeGitHost::new()
                    .with_open_prs("octo/widget", vec![summary("17", "sha17", "First PR")])
                    .fail_next("list_open_prs"),
            )),
            gitlab: None,
        };
        let second = collector.sync(Some(tmp.path()), &failing_hosts, None).await;
        assert_eq!(second.snapshot.pull_requests.len(), 1);
        assert!(second.snapshot.outdated);
        assert!(second.source_status.last_sync_error.is_some());
    }
}
