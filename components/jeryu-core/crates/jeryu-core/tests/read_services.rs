//! Contract tests for the transport-neutral forge read boundary.

use chrono::Utc;
use jeryu_core::{
    AuditEntry, AuditReadService, BranchProtectionReadService, BranchProtectionRule,
    CheckReadService, CheckRunList, CreateCheckRunRequest, CreatePullRequestRequest,
    CreateRepositoryRequest, ForgeCore, ForgeError, ForgeReadService, PullRequest,
    PullRequestReadService, PullRequestState, Repository, RepositoryReadService, Result,
};
use uuid::Uuid;

fn repository_names(service: &dyn ForgeReadService) -> Result<Vec<String>> {
    Ok(service
        .list_repositories(None)?
        .into_iter()
        .map(|repository| repository.full_name)
        .collect())
}

#[derive(Debug)]
struct FakeReadService {
    repository: Repository,
}

impl FakeReadService {
    fn new() -> Self {
        let now = Utc::now();
        Self {
            repository: Repository {
                id: Uuid::new_v4(),
                owner: "veox".to_string(),
                name: "fake".to_string(),
                full_name: "veox/fake".to_string(),
                private: true,
                description: None,
                default_branch: "main".to_string(),
                family: Some("jeryu-split".to_string()),
                archived: false,
                disabled: false,
                created_at: now,
                updated_at: now,
            },
        }
    }
}

impl RepositoryReadService for FakeReadService {
    fn list_repositories(&self, owner: Option<&str>) -> Result<Vec<Repository>> {
        Ok(owner
            .is_none_or(|owner| owner == self.repository.owner)
            .then(|| self.repository.clone())
            .into_iter()
            .collect())
    }

    fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        if owner == self.repository.owner && repo == self.repository.name {
            Ok(self.repository.clone())
        } else {
            Err(ForgeError::NotFound(format!("repository {owner}/{repo}")))
        }
    }
}

impl PullRequestReadService for FakeReadService {
    fn list_pull_requests(
        &self,
        _owner: &str,
        _repo: &str,
        _state: Option<PullRequestState>,
    ) -> Result<Vec<PullRequest>> {
        Ok(Vec::new())
    }

    fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> Result<PullRequest> {
        Err(ForgeError::NotFound(format!(
            "pull request {owner}/{repo}#{number}"
        )))
    }
}

impl CheckReadService for FakeReadService {
    fn list_check_runs(
        &self,
        _owner: &str,
        _repo: &str,
        _head_sha: Option<&str>,
    ) -> Result<CheckRunList> {
        Ok(CheckRunList {
            total_count: 0,
            check_runs: Vec::new(),
        })
    }
}

impl BranchProtectionReadService for FakeReadService {
    fn get_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<BranchProtectionRule> {
        Err(ForgeError::NotFound(format!(
            "branch protection {owner}/{repo}:{branch}"
        )))
    }
}

impl AuditReadService for FakeReadService {
    fn list_audit(&self, _subject: &str) -> Result<Vec<AuditEntry>> {
        Ok(Vec::new())
    }
}

#[test]
fn consumer_can_use_an_in_memory_fake_without_forge_core() {
    let fake = FakeReadService::new();

    assert_eq!(repository_names(&fake).unwrap(), ["veox/fake"]);
    assert!(matches!(
        fake.get_repository("veox", "missing"),
        Err(ForgeError::NotFound(_))
    ));
}

#[test]
fn forge_core_satisfies_the_same_object_safe_contract() {
    let core = ForgeCore::new();
    core.create_repository(
        "veox",
        CreateRepositoryRequest {
            name: "jeryu-core".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    let pull = core
        .create_pull_request(
            "veox",
            "jeryu-core",
            "agent",
            CreatePullRequestRequest {
                title: "transport-neutral reads".to_string(),
                head: "agents/agent/session".to_string(),
                base: "main".to_string(),
                head_sha: Some("abc123".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    core.create_check_run(
        "veox",
        "jeryu-core",
        CreateCheckRunRequest {
            name: "jeryu-core/required".to_string(),
            head_sha: "abc123".to_string(),
            ..Default::default()
        },
    )
    .unwrap();

    let service: &dyn ForgeReadService = &core;
    assert_eq!(repository_names(service).unwrap(), ["veox/jeryu-core"]);
    assert_eq!(
        service
            .get_pull_request("veox", "jeryu-core", pull.number)
            .unwrap()
            .number,
        pull.number
    );
    assert_eq!(
        service
            .list_check_runs("veox", "jeryu-core", Some("abc123"))
            .unwrap()
            .total_count,
        1
    );
    assert_eq!(
        service
            .get_branch_protection("veox", "jeryu-core", "main")
            .unwrap()
            .branch,
        "main"
    );
    assert!(service.list_audit("veox/jeryu-core").unwrap().is_empty());
}
