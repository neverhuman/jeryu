use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::branch_protection::{
    BranchProtectionEvaluation, EvaluationContext, RefOperation, RefOperationEvaluation,
    evaluate_branch_protection_with, evaluate_ref_operation,
};
use crate::errors::{ForgeError, Result};
use crate::model::*;
use crate::webhooks::{event_payload, should_deliver, sign_webhook_payload};

#[derive(Debug, Default)]
struct Counters {
    issue: u64,
    pull: u64,
}

#[derive(Debug, Default)]
struct State {
    users: HashMap<String, User>,
    organizations: HashMap<String, Organization>,
    teams: HashMap<(String, String), Team>,
    repos: HashMap<(String, String), Repository>,
    labels: HashMap<(String, String, String), Label>,
    issues: HashMap<(String, String, u64), Issue>,
    issue_comments: HashMap<(String, String, u64), Vec<IssueComment>>,
    pulls: HashMap<(String, String, u64), PullRequest>,
    reviews: HashMap<(String, String, u64), Vec<Review>>,
    review_comments: HashMap<(String, String, u64), Vec<ReviewComment>>,
    branch_protections: HashMap<(String, String, String), BranchProtectionRule>,
    codeowners: HashMap<(String, String), String>,
    statuses: HashMap<(String, String, String), Vec<CommitStatus>>,
    check_runs: HashMap<(String, String), Vec<CheckRun>>,
    webhooks: HashMap<(String, String), Vec<Webhook>>,
    webhook_deliveries: Vec<WebhookDelivery>,
    counters: HashMap<(String, String), Counters>,
}

#[derive(Debug, Clone, Default)]
pub struct ForgeCore {
    state: Arc<RwLock<State>>,
}

impl ForgeCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_user(&self, request: CreateUserRequest) -> Result<User> {
        require_name("login", &request.login)?;
        let mut state = self.state.write();
        if state.users.contains_key(&request.login) {
            return Err(ForgeError::Conflict(format!("user {}", request.login)));
        }
        let user = User {
            id: Uuid::new_v4(),
            login: request.login.clone(),
            name: request.name,
            email: request.email,
            created_at: Utc::now(),
        };
        state.users.insert(request.login, user.clone());
        Ok(user)
    }

    pub fn get_user(&self, login: &str) -> Result<User> {
        self.state
            .read()
            .users
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("user {login}")))
    }

    pub fn ensure_user(&self, login: &str) -> User {
        if let Ok(user) = self.get_user(login) {
            return user;
        }
        self.create_user(CreateUserRequest {
            login: login.to_string(),
            name: None,
            email: None,
        })
        .expect("ensure_user creates a unique user after not-found")
    }

    pub fn create_organization(&self, request: CreateOrganizationRequest) -> Result<Organization> {
        require_name("login", &request.login)?;
        let mut state = self.state.write();
        if state.organizations.contains_key(&request.login) {
            return Err(ForgeError::Conflict(format!(
                "organization {}",
                request.login
            )));
        }
        let organization = Organization {
            id: Uuid::new_v4(),
            login: request.login.clone(),
            display_name: request.display_name,
            created_at: Utc::now(),
        };
        state
            .organizations
            .insert(request.login, organization.clone());
        Ok(organization)
    }

    pub fn get_organization(&self, login: &str) -> Result<Organization> {
        self.state
            .read()
            .organizations
            .get(login)
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("organization {login}")))
    }

    pub fn create_team(&self, org: &str, request: CreateTeamRequest) -> Result<Team> {
        require_name("team name", &request.name)?;
        let slug = request.slug.unwrap_or_else(|| slugify(&request.name));
        require_name("team slug", &slug)?;
        let mut state = self.state.write();
        if !state.organizations.contains_key(org) {
            return Err(ForgeError::NotFound(format!("organization {org}")));
        }
        let key = (org.to_string(), slug.clone());
        if state.teams.contains_key(&key) {
            return Err(ForgeError::Conflict(format!("team {org}/{slug}")));
        }
        let team = Team {
            id: Uuid::new_v4(),
            organization: org.to_string(),
            slug: slug.clone(),
            name: request.name,
            members: request.members,
            created_at: Utc::now(),
        };
        state.teams.insert(key, team.clone());
        Ok(team)
    }

    pub fn list_teams(&self, org: &str) -> Result<Vec<Team>> {
        let state = self.state.read();
        if !state.organizations.contains_key(org) {
            return Err(ForgeError::NotFound(format!("organization {org}")));
        }
        Ok(state
            .teams
            .values()
            .filter(|team| team.organization == org)
            .cloned()
            .collect())
    }

    pub fn create_repository(
        &self,
        owner: &str,
        request: CreateRepositoryRequest,
    ) -> Result<Repository> {
        require_name("repository name", &request.name)?;
        let mut state = self.state.write();
        let key = (owner.to_string(), request.name.clone());
        if state.repos.contains_key(&key) {
            return Err(ForgeError::Conflict(format!(
                "repository {owner}/{}",
                request.name
            )));
        }
        let now = Utc::now();
        let repo = Repository {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            name: request.name.clone(),
            full_name: format!("{owner}/{}", request.name),
            private: request.private,
            description: request.description,
            default_branch: request.default_branch.unwrap_or_else(|| "main".to_string()),
            archived: false,
            disabled: false,
            created_at: now,
            updated_at: now,
        };
        state.counters.insert(key.clone(), Counters::default());
        state.repos.insert(key, repo.clone());
        Ok(repo)
    }

    pub fn list_repositories(&self, owner: Option<&str>) -> Vec<Repository> {
        let mut repos: Vec<_> = self
            .state
            .read()
            .repos
            .values()
            .filter(|repo| owner.is_none_or(|owner| repo.owner == owner))
            .cloned()
            .collect();
        repos.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        repos
    }

    pub fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository> {
        self.state
            .read()
            .repos
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("repository {owner}/{repo}")))
    }

    pub fn create_label(
        &self,
        owner: &str,
        repo: &str,
        request: CreateLabelRequest,
    ) -> Result<Label> {
        require_name("label name", &request.name)?;
        self.ensure_repo_exists(owner, repo)?;
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), request.name.clone());
        if state.labels.contains_key(&key) {
            return Err(ForgeError::Conflict(format!(
                "label {owner}/{repo}/{}",
                request.name
            )));
        }
        let label = Label {
            id: Uuid::new_v4(),
            name: request.name,
            color: request.color,
            description: request.description,
        };
        state.labels.insert(key, label.clone());
        Ok(label)
    }

    pub fn list_labels(&self, owner: &str, repo: &str) -> Result<Vec<Label>> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
        let mut labels: Vec<_> = state
            .labels
            .iter()
            .filter(|((label_owner, label_repo, _), _)| label_owner == owner && label_repo == repo)
            .map(|(_, label)| label.clone())
            .collect();
        labels.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(labels)
    }

    pub fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        request: CreateIssueRequest,
    ) -> Result<Issue> {
        require_name("issue title", &request.title)?;
        self.ensure_repo_exists(owner, repo)?;
        self.ensure_user(author);
        let mut state = self.state.write();
        let number = next_issue_number(&mut state, owner, repo);
        let now = Utc::now();
        let issue = Issue {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            title: request.title,
            body: request.body,
            state: IssueState::Open,
            author: author.to_string(),
            labels: request.labels,
            assignees: request.assignees,
            milestone: request.milestone,
            comments: 0,
            pull_request: None,
            created_at: now,
            updated_at: now,
            closed_at: None,
        };
        state
            .issues
            .insert((owner.to_string(), repo.to_string(), number), issue.clone());
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "issues",
            event_payload("opened", "issue", json!(issue.clone())),
        );
        Ok(issue)
    }

    pub fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state_filter: Option<IssueState>,
    ) -> Result<Vec<Issue>> {
        self.ensure_repo_exists(owner, repo)?;
        let mut issues: Vec<_> = self
            .state
            .read()
            .issues
            .values()
            .filter(|issue| issue.owner == owner && issue.repo == repo)
            .filter(|issue| {
                state_filter
                    .as_ref()
                    .is_none_or(|state| &issue.state == state)
            })
            .cloned()
            .collect();
        issues.sort_by_key(|issue| issue.number);
        Ok(issues)
    }

    pub fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Issue> {
        self.state
            .read()
            .issues
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("issue {owner}/{repo}#{number}")))
    }

    pub fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        request: UpdateIssueRequest,
    ) -> Result<Issue> {
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), number);
        let issue = state
            .issues
            .get_mut(&key)
            .ok_or_else(|| ForgeError::NotFound(format!("issue {owner}/{repo}#{number}")))?;
        if let Some(title) = request.title {
            require_name("issue title", &title)?;
            issue.title = title;
        }
        if request.body.is_some() {
            issue.body = request.body;
        }
        if let Some(labels) = request.labels {
            issue.labels = labels;
        }
        if let Some(assignees) = request.assignees {
            issue.assignees = assignees;
        }
        if request.milestone.is_some() {
            issue.milestone = request.milestone;
        }
        let action = if let Some(new_state) = request.state {
            issue.state = new_state.clone();
            issue.closed_at = if new_state == IssueState::Closed {
                Some(Utc::now())
            } else {
                None
            };
            match new_state {
                IssueState::Open => "reopened",
                IssueState::Closed => "closed",
            }
        } else {
            "edited"
        };
        issue.updated_at = Utc::now();
        let updated = issue.clone();
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "issues",
            event_payload(action, "issue", json!(updated.clone())),
        );
        Ok(updated)
    }

    pub fn add_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        request: CreateCommentRequest,
    ) -> Result<IssueComment> {
        require_name("comment body", &request.body)?;
        self.ensure_user(author);
        let mut state = self.state.write();
        let issue_key = (owner.to_string(), repo.to_string(), number);
        let issue = state
            .issues
            .get_mut(&issue_key)
            .ok_or_else(|| ForgeError::NotFound(format!("issue {owner}/{repo}#{number}")))?;
        issue.comments += 1;
        issue.updated_at = Utc::now();
        let comment = IssueComment {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            issue_number: number,
            author: author.to_string(),
            body: request.body,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state
            .issue_comments
            .entry(issue_key)
            .or_default()
            .push(comment.clone());
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "issue_comment",
            event_payload("created", "comment", json!(comment.clone())),
        );
        Ok(comment)
    }

    pub fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<IssueComment>> {
        self.get_issue(owner, repo, number)?;
        Ok(self
            .state
            .read()
            .issue_comments
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .unwrap_or_default())
    }

    pub fn create_pull_request(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        request: CreatePullRequestRequest,
    ) -> Result<PullRequest> {
        require_name("pull request title", &request.title)?;
        require_name("head", &request.head)?;
        require_name("base", &request.base)?;
        self.ensure_repo_exists(owner, repo)?;
        self.ensure_user(author);
        let mut state = self.state.write();
        let issue_number = next_issue_number(&mut state, owner, repo);
        let pull_number = next_pull_number(&mut state, owner, repo);
        let now = Utc::now();
        let issue = Issue {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: issue_number,
            title: request.title.clone(),
            body: request.body.clone(),
            state: IssueState::Open,
            author: author.to_string(),
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
            comments: 0,
            pull_request: Some(PullRequestMarker {
                url: format!("/repos/{owner}/{repo}/pulls/{pull_number}"),
                html_url: format!("/{owner}/{repo}/pull/{pull_number}"),
            }),
            created_at: now,
            updated_at: now,
            closed_at: None,
        };
        let mut pr = PullRequest {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: pull_number,
            issue_number,
            title: request.title,
            body: request.body,
            state: if request.draft {
                PullRequestState::Draft
            } else {
                PullRequestState::Open
            },
            draft: request.draft,
            author: author.to_string(),
            head: GitBranchRef::new(
                request.head,
                request
                    .head_sha
                    .unwrap_or_else(|| format!("head-{pull_number}")),
            ),
            base: GitBranchRef::new(
                request.base,
                request.base_sha.unwrap_or_else(|| "base".to_string()),
            ),
            mergeable: false,
            mergeable_state: "unknown".to_string(),
            merged: false,
            merged_at: None,
            merge_commit_sha: None,
            commits: request.commits,
            changed_files: request.changed_files,
            created_at: now,
            updated_at: now,
        };
        let evaluation = evaluate_locked(&state, &pr, None);
        apply_evaluation(&mut pr, evaluation);
        state
            .issues
            .insert((owner.to_string(), repo.to_string(), issue_number), issue);
        state.pulls.insert(
            (owner.to_string(), repo.to_string(), pull_number),
            pr.clone(),
        );
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "pull_request",
            event_payload("opened", "pull_request", json!(pr.clone())),
        );
        Ok(pr)
    }

    pub fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state_filter: Option<PullRequestState>,
    ) -> Result<Vec<PullRequest>> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
        let mut pulls: Vec<_> = state
            .pulls
            .values()
            .filter(|pr| pr.owner == owner && pr.repo == repo)
            .filter(|pr| {
                state_filter
                    .as_ref()
                    .is_none_or(|filter| &pr.state == filter)
            })
            .map(|pr| {
                let mut pr = pr.clone();
                let evaluation = evaluate_locked(&state, &pr, None);
                apply_evaluation(&mut pr, evaluation);
                pr
            })
            .collect();
        pulls.sort_by_key(|pr| pr.number);
        Ok(pulls)
    }

    pub fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> Result<PullRequest> {
        let state = self.state.read();
        let mut pr = state
            .pulls
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?;
        let evaluation = evaluate_locked(&state, &pr, None);
        apply_evaluation(&mut pr, evaluation);
        Ok(pr)
    }

    pub fn update_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        request: UpdatePullRequestRequest,
    ) -> Result<PullRequest> {
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), number);
        let pr = state
            .pulls
            .get_mut(&key)
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?;
        if let Some(title) = request.title {
            require_name("pull request title", &title)?;
            pr.title = title;
        }
        if request.body.is_some() {
            pr.body = request.body;
        }
        if let Some(draft) = request.draft {
            pr.draft = draft;
            pr.state = if draft {
                PullRequestState::Draft
            } else {
                PullRequestState::Open
            };
        }
        if let Some(state_update) = request.state {
            pr.state = state_update;
            pr.draft = pr.state == PullRequestState::Draft;
        }
        if let Some(commits) = request.commits {
            pr.commits = commits;
        }
        if let Some(changed_files) = request.changed_files {
            pr.changed_files = changed_files;
        }
        pr.updated_at = Utc::now();
        let mut updated = pr.clone();
        let evaluation = evaluate_locked(&state, &updated, None);
        apply_evaluation(&mut updated, evaluation);
        state.pulls.insert(key, updated.clone());
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "pull_request",
            event_payload("edited", "pull_request", json!(updated.clone())),
        );
        Ok(updated)
    }

    pub fn create_review(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        request: CreateReviewRequest,
    ) -> Result<Review> {
        self.ensure_user(author);
        let mut state = self.state.write();
        if !state
            .pulls
            .contains_key(&(owner.to_string(), repo.to_string(), number))
        {
            return Err(ForgeError::NotFound(format!(
                "pull request {owner}/{repo}#{number}"
            )));
        }
        let review_id = Uuid::new_v4();
        let review = Review {
            id: review_id,
            owner: owner.to_string(),
            repo: repo.to_string(),
            pull_number: number,
            author: author.to_string(),
            state: request.event,
            body: request.body,
            submitted_at: Utc::now(),
        };
        let comments: Vec<_> = request
            .comments
            .into_iter()
            .map(|comment| ReviewComment {
                id: Uuid::new_v4(),
                review_id,
                owner: owner.to_string(),
                repo: repo.to_string(),
                pull_number: number,
                path: comment.path,
                line: comment.line,
                author: author.to_string(),
                body: comment.body,
                created_at: Utc::now(),
            })
            .collect();
        state
            .reviews
            .entry((owner.to_string(), repo.to_string(), number))
            .or_default()
            .push(review.clone());
        state
            .review_comments
            .entry((owner.to_string(), repo.to_string(), number))
            .or_default()
            .extend(comments);
        if let Some(pr) = state
            .pulls
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
        {
            let mut updated = pr;
            let evaluation = evaluate_locked(&state, &updated, None);
            apply_evaluation(&mut updated, evaluation);
            state
                .pulls
                .insert((owner.to_string(), repo.to_string(), number), updated);
        }
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "pull_request_review",
            event_payload("submitted", "review", json!(review.clone())),
        );
        Ok(review)
    }

    pub fn list_reviews(&self, owner: &str, repo: &str, number: u64) -> Result<Vec<Review>> {
        self.get_pull_request(owner, repo, number)?;
        Ok(self
            .state
            .read()
            .reviews
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .unwrap_or_default())
    }

    pub fn list_review_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<ReviewComment>> {
        self.get_pull_request(owner, repo, number)?;
        Ok(self
            .state
            .read()
            .review_comments
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .unwrap_or_default())
    }

    pub fn set_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        request: SetBranchProtectionRequest,
    ) -> Result<BranchProtectionRule> {
        self.ensure_repo_exists(owner, repo)?;
        require_name("branch", branch)?;
        let rule = BranchProtectionRule {
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
            required_status_checks: request.required_status_checks,
            required_approving_review_count: request.required_approving_review_count,
            enforce_admins: request.enforce_admins,
            required_linear_history: request.required_linear_history,
            allow_force_pushes: request.allow_force_pushes,
            allow_deletions: request.allow_deletions,
            require_signed_commits: request.require_signed_commits,
            require_jankurai_proof: request.require_jankurai_proof,
            updated_at: Utc::now(),
        };
        self.state.write().branch_protections.insert(
            (owner.to_string(), repo.to_string(), branch.to_string()),
            rule.clone(),
        );
        Ok(rule)
    }

    pub fn get_branch_protection(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<BranchProtectionRule> {
        self.state
            .read()
            .branch_protections
            .get(&(owner.to_string(), repo.to_string(), branch.to_string()))
            .cloned()
            .ok_or_else(|| {
                ForgeError::NotFound(format!("branch protection {owner}/{repo}:{branch}"))
            })
    }

    /// Stores the repository's CODEOWNERS file contents. Used by branch
    /// protection to require code-owner approval of changed paths.
    pub fn set_codeowners(&self, owner: &str, repo: &str, contents: &str) -> Result<()> {
        self.ensure_repo_exists(owner, repo)?;
        self.state
            .write()
            .codeowners
            .insert((owner.to_string(), repo.to_string()), contents.to_string());
        Ok(())
    }

    pub fn get_codeowners(&self, owner: &str, repo: &str) -> Result<String> {
        self.ensure_repo_exists(owner, repo)?;
        self.state
            .read()
            .codeowners
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("CODEOWNERS {owner}/{repo}")))
    }

    pub fn evaluate_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        sha: Option<&str>,
    ) -> Result<BranchProtectionEvaluation> {
        let state = self.state.read();
        let pr = state
            .pulls
            .get(&(owner.to_string(), repo.to_string(), number))
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?;
        Ok(evaluate_locked(&state, pr, sha))
    }

    /// Evaluates a force-push or branch-deletion against the branch's
    /// protection rule. `actor_is_admin` lets an admin bypass when the rule's
    /// `enforce_admins` is off (GitHub's "Include administrators" toggle).
    pub fn evaluate_ref_operation(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        operation: RefOperation,
        actor_is_admin: bool,
    ) -> Result<RefOperationEvaluation> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
        let protection = state.branch_protections.get(&(
            owner.to_string(),
            repo.to_string(),
            branch.to_string(),
        ));
        Ok(evaluate_ref_operation(
            operation,
            protection,
            EvaluationContext {
                codeowners: None,
                actor_is_admin,
            },
        ))
    }

    /// Attempts to force-push the branch, honoring `allow_force_pushes`.
    pub fn force_push(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        actor_is_admin: bool,
    ) -> Result<()> {
        let evaluation = self.evaluate_ref_operation(
            owner,
            repo,
            branch,
            RefOperation::ForcePush,
            actor_is_admin,
        )?;
        if evaluation.allowed {
            Ok(())
        } else {
            Err(ForgeError::BranchProtection(format!(
                "{:?}",
                evaluation.blockers
            )))
        }
    }

    /// Attempts to delete the branch ref, honoring `allow_deletions`.
    pub fn delete_ref(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        actor_is_admin: bool,
    ) -> Result<()> {
        let evaluation =
            self.evaluate_ref_operation(owner, repo, branch, RefOperation::Delete, actor_is_admin)?;
        if evaluation.allowed {
            Ok(())
        } else {
            Err(ForgeError::BranchProtection(format!(
                "{:?}",
                evaluation.blockers
            )))
        }
    }

    pub fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        request: MergePullRequestRequest,
    ) -> Result<MergeResult> {
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), number);
        let pr_snapshot =
            state.pulls.get(&key).cloned().ok_or_else(|| {
                ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}"))
            })?;
        if pr_snapshot.merged {
            return Ok(MergeResult {
                sha: pr_snapshot
                    .merge_commit_sha
                    .unwrap_or_else(|| pr_snapshot.head.sha.clone()),
                merged: true,
                message: "Pull Request already merged".to_string(),
            });
        }
        if pr_snapshot.state == PullRequestState::Closed {
            return Err(ForgeError::Validation(
                "closed pull requests cannot be merged".to_string(),
            ));
        }
        let evaluation = evaluate_locked(&state, &pr_snapshot, request.sha.as_deref());
        if !evaluation.mergeable {
            return Err(ForgeError::BranchProtection(format!(
                "{:?}",
                evaluation.blockers
            )));
        }
        let merge_sha = format!("merge-{}-{}", pr_snapshot.head.sha, number);
        let mut pr = pr_snapshot;
        pr.merged = true;
        pr.state = PullRequestState::Merged;
        pr.mergeable = false;
        pr.mergeable_state = "merged".to_string();
        pr.merged_at = Some(Utc::now());
        pr.merge_commit_sha = Some(merge_sha.clone());
        pr.updated_at = Utc::now();
        state.pulls.insert(key, pr.clone());
        if let Some(issue) =
            state
                .issues
                .get_mut(&(owner.to_string(), repo.to_string(), pr.issue_number))
        {
            issue.state = IssueState::Closed;
            issue.closed_at = Some(Utc::now());
            issue.updated_at = Utc::now();
        }
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "pull_request",
            event_payload("closed", "pull_request", json!(pr)),
        );
        Ok(MergeResult {
            sha: merge_sha,
            merged: true,
            message: "Pull Request successfully merged".to_string(),
        })
    }

    pub fn create_commit_status(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        creator: &str,
        request: CreateCommitStatusRequest,
    ) -> Result<CommitStatus> {
        require_name("sha", sha)?;
        self.ensure_repo_exists(owner, repo)?;
        self.ensure_user(creator);
        let now = Utc::now();
        let status = CommitStatus {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            sha: sha.to_string(),
            state: request.state,
            context: request.context,
            description: request.description,
            target_url: request.target_url,
            creator: creator.to_string(),
            created_at: now,
            updated_at: now,
        };
        let mut state = self.state.write();
        state
            .statuses
            .entry((owner.to_string(), repo.to_string(), sha.to_string()))
            .or_default()
            .push(status.clone());
        refresh_pull_mergeability_for_sha(&mut state, owner, repo, sha);
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "status",
            event_payload("created", "status", json!(status.clone())),
        );
        Ok(status)
    }

    pub fn combined_status(&self, owner: &str, repo: &str, sha: &str) -> Result<CombinedStatus> {
        self.ensure_repo_exists(owner, repo)?;
        let statuses = self
            .state
            .read()
            .statuses
            .get(&(owner.to_string(), repo.to_string(), sha.to_string()))
            .cloned()
            .unwrap_or_default();
        let state = if statuses
            .iter()
            .any(|status| status.state == CommitStatusState::Error)
        {
            CommitStatusState::Error
        } else if statuses
            .iter()
            .any(|status| status.state == CommitStatusState::Failure)
        {
            CommitStatusState::Failure
        } else if statuses.is_empty()
            || statuses
                .iter()
                .any(|status| status.state == CommitStatusState::Pending)
        {
            CommitStatusState::Pending
        } else {
            CommitStatusState::Success
        };
        Ok(CombinedStatus {
            state,
            total_count: statuses.len(),
            statuses,
        })
    }

    pub fn create_check_run(
        &self,
        owner: &str,
        repo: &str,
        request: CreateCheckRunRequest,
    ) -> Result<CheckRun> {
        require_name("check run name", &request.name)?;
        require_name("head sha", &request.head_sha)?;
        self.ensure_repo_exists(owner, repo)?;
        let completed = matches!(request.status, Some(CheckRunStatus::Completed))
            || request.conclusion.is_some();
        let check_run = CheckRun {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: request.name,
            head_sha: request.head_sha,
            status: request.status.unwrap_or(if completed {
                CheckRunStatus::Completed
            } else {
                CheckRunStatus::Queued
            }),
            conclusion: request.conclusion,
            details_url: request.details_url,
            output: request.output,
            started_at: Utc::now(),
            completed_at: if completed { Some(Utc::now()) } else { None },
        };
        let mut state = self.state.write();
        state
            .check_runs
            .entry((owner.to_string(), repo.to_string()))
            .or_default()
            .push(check_run.clone());
        refresh_pull_mergeability_for_sha(&mut state, owner, repo, &check_run.head_sha);
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "check_run",
            event_payload("created", "check_run", json!(check_run.clone())),
        );
        Ok(check_run)
    }

    pub fn list_check_runs(
        &self,
        owner: &str,
        repo: &str,
        head_sha: Option<&str>,
    ) -> Result<CheckRunList> {
        self.ensure_repo_exists(owner, repo)?;
        let runs: Vec<_> = self
            .state
            .read()
            .check_runs
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|run| head_sha.is_none_or(|sha| run.head_sha == sha))
            .collect();
        Ok(CheckRunList {
            total_count: runs.len(),
            check_runs: runs,
        })
    }
    pub fn check_suites(
        &self,
        owner: &str,
        repo: &str,
        head_sha: Option<&str>,
    ) -> Result<Vec<CheckSuite>> {
        let runs = self.list_check_runs(owner, repo, head_sha)?.check_runs;
        let mut grouped: HashMap<String, Vec<CheckRun>> = HashMap::new();
        for run in runs {
            grouped.entry(run.head_sha.clone()).or_default().push(run);
        }
        Ok(grouped
            .into_iter()
            .map(|(sha, runs)| {
                let status = if runs
                    .iter()
                    .all(|run| run.status == CheckRunStatus::Completed)
                {
                    CheckRunStatus::Completed
                } else if runs
                    .iter()
                    .any(|run| run.status == CheckRunStatus::InProgress)
                {
                    CheckRunStatus::InProgress
                } else {
                    CheckRunStatus::Queued
                };
                let conclusion = if status == CheckRunStatus::Completed {
                    if runs
                        .iter()
                        .all(|run| run.conclusion == Some(CheckConclusion::Success))
                    {
                        Some(CheckConclusion::Success)
                    } else {
                        Some(CheckConclusion::Failure)
                    }
                } else {
                    None
                };
                CheckSuite {
                    id: Uuid::new_v4(),
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                    head_sha: sha,
                    status,
                    conclusion,
                    check_runs: runs,
                }
            })
            .collect())
    }

    pub fn workflow_runs(&self, owner: &str, repo: &str) -> Result<WorkflowRunList> {
        let suites = self.check_suites(owner, repo, None)?;
        let workflow_runs: Vec<_> = suites
            .into_iter()
            .map(|suite| WorkflowRun {
                id: suite.id,
                name: "Phase 2 checks".to_string(),
                head_sha: suite.head_sha,
                status: suite.status,
                conclusion: suite.conclusion,
                check_runs: suite.check_runs,
            })
            .collect();
        Ok(WorkflowRunList {
            total_count: workflow_runs.len(),
            workflow_runs,
        })
    }

    pub fn create_webhook(
        &self,
        owner: &str,
        repo: &str,
        request: CreateWebhookRequest,
    ) -> Result<Webhook> {
        self.ensure_repo_exists(owner, repo)?;
        require_name("webhook url", &request.config.url)?;
        let now = Utc::now();
        let hook = Webhook {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: request.name,
            active: request.active,
            events: request.events,
            config: request.config,
            created_at: now,
            updated_at: now,
        };
        self.state
            .write()
            .webhooks
            .entry((owner.to_string(), repo.to_string()))
            .or_default()
            .push(hook.clone());
        Ok(hook)
    }

    pub fn list_webhooks(&self, owner: &str, repo: &str) -> Result<Vec<Webhook>> {
        self.ensure_repo_exists(owner, repo)?;
        Ok(self
            .state
            .read()
            .webhooks
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    pub fn list_webhook_deliveries(&self, owner: &str, repo: &str) -> Result<Vec<WebhookDelivery>> {
        self.ensure_repo_exists(owner, repo)?;
        Ok(self
            .state
            .read()
            .webhook_deliveries
            .iter()
            .filter(|delivery| delivery.owner == owner && delivery.repo == repo)
            .cloned()
            .collect())
    }

    pub fn app_installations(&self) -> InstallationList {
        InstallationList {
            total_count: 0,
            installations: Vec::new(),
        }
    }

    fn ensure_repo_exists(&self, owner: &str, repo: &str) -> Result<()> {
        self.get_repository(owner, repo).map(|_| ())
    }
}

fn require_name(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(ForgeError::Validation(format!("{field} cannot be empty")))
    } else {
        Ok(())
    }
}

fn slugify(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn next_issue_number(state: &mut State, owner: &str, repo: &str) -> u64 {
    let counters = state
        .counters
        .entry((owner.to_string(), repo.to_string()))
        .or_default();
    counters.issue += 1;
    counters.issue
}

fn next_pull_number(state: &mut State, owner: &str, repo: &str) -> u64 {
    let counters = state
        .counters
        .entry((owner.to_string(), repo.to_string()))
        .or_default();
    counters.pull += 1;
    counters.pull
}

fn evaluate_locked(
    state: &State,
    pr: &PullRequest,
    requested_sha: Option<&str>,
) -> BranchProtectionEvaluation {
    let protection = state.branch_protections.get(&(
        pr.owner.clone(),
        pr.repo.clone(),
        pr.base.ref_name.clone(),
    ));
    let reviews = state
        .reviews
        .get(&(pr.owner.clone(), pr.repo.clone(), pr.number))
        .cloned()
        .unwrap_or_default();
    let statuses = state
        .statuses
        .get(&(pr.owner.clone(), pr.repo.clone(), pr.head.sha.clone()))
        .cloned()
        .unwrap_or_default();
    let check_runs = state
        .check_runs
        .get(&(pr.owner.clone(), pr.repo.clone()))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|check| check.head_sha == pr.head.sha)
        .collect::<Vec<_>>();
    let codeowners = state.codeowners.get(&(pr.owner.clone(), pr.repo.clone()));
    let context = EvaluationContext {
        codeowners: codeowners.map(String::as_str),
        actor_is_admin: false,
    };
    evaluate_branch_protection_with(
        pr,
        protection,
        &reviews,
        &statuses,
        &check_runs,
        requested_sha,
        context,
    )
}

fn apply_evaluation(pr: &mut PullRequest, evaluation: BranchProtectionEvaluation) {
    // Terminal states are sticky. GitHub never resurrects a Merged or Closed PR
    // by recomputing mergeability on read: a merged PR stays merged, and a
    // closed PR stays closed until it is explicitly reopened. Previously only
    // `merged` was sticky, so a Closed PR with no blocking protection was
    // silently reverted to Mergeable on the next read (pinned by the former
    // `closing_a_mergeable_pr_does_not_stick`). This is the deliberate
    // correctness fix.
    if pr.merged || pr.state == PullRequestState::Merged {
        pr.mergeable = false;
        pr.mergeable_state = "merged".to_string();
        return;
    }
    if pr.state == PullRequestState::Closed {
        pr.mergeable = false;
        pr.mergeable_state = "closed".to_string();
        return;
    }
    pr.mergeable = evaluation.mergeable;
    pr.mergeable_state = evaluation.state;
    if pr.draft {
        pr.state = PullRequestState::Draft;
    } else if evaluation.mergeable {
        pr.state = PullRequestState::Mergeable;
    } else {
        pr.state = PullRequestState::BlockedByChecks;
    }
}

fn refresh_pull_mergeability_for_sha(state: &mut State, owner: &str, repo: &str, sha: &str) {
    let keys: Vec<_> = state
        .pulls
        .iter()
        .filter(|((pull_owner, pull_repo, _), pr)| {
            pull_owner == owner && pull_repo == repo && pr.head.sha == sha
        })
        .map(|(key, _)| key.clone())
        .collect();

    for key in keys {
        if let Some(snapshot) = state.pulls.get(&key).cloned() {
            let mut updated = snapshot;
            let evaluation = evaluate_locked(state, &updated, None);
            apply_evaluation(&mut updated, evaluation);
            state.pulls.insert(key, updated);
        }
    }
}

fn emit_event_locked(state: &mut State, owner: &str, repo: &str, event: &str, payload: Value) {
    let hooks = state
        .webhooks
        .get(&(owner.to_string(), repo.to_string()))
        .cloned()
        .unwrap_or_default();
    for hook in hooks.iter().filter(|hook| should_deliver(hook, event)) {
        let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();
        let signature_256 = hook
            .config
            .secret
            .as_ref()
            .map(|secret| sign_webhook_payload(secret, &payload_bytes));
        state.webhook_deliveries.push(WebhookDelivery {
            id: Uuid::new_v4(),
            hook_id: hook.id,
            owner: owner.to_string(),
            repo: repo.to_string(),
            event: event.to_string(),
            target_url: hook.config.url.clone(),
            payload: payload.clone(),
            signature_256,
            delivered: false,
            created_at: Utc::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with_repo() -> ForgeCore {
        let core = ForgeCore::new();
        core.create_user(CreateUserRequest {
            login: "alice".to_string(),
            name: None,
            email: None,
        })
        .unwrap();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: Some("forge".to_string()),
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        core
    }

    #[test]
    fn lifecycle_issue_and_comment() {
        let core = core_with_repo();
        let issue = core
            .create_issue(
                "alice",
                "jeryu",
                "alice",
                CreateIssueRequest {
                    title: "bug".to_string(),
                    body: Some("fix it".to_string()),
                    labels: vec!["bug".to_string()],
                    assignees: Vec::new(),
                    milestone: None,
                },
            )
            .unwrap();
        assert_eq!(issue.number, 1);
        let comment = core
            .add_issue_comment(
                "alice",
                "jeryu",
                issue.number,
                "alice",
                CreateCommentRequest {
                    body: "confirmed".to_string(),
                },
            )
            .unwrap();
        assert_eq!(comment.issue_number, issue.number);
        assert_eq!(
            core.get_issue("alice", "jeryu", issue.number)
                .unwrap()
                .comments,
            1
        );
    }

    #[test]
    fn branch_protection_blocks_merge_until_review_and_status_pass() {
        let core = core_with_repo();
        core.set_branch_protection(
            "alice",
            "jeryu",
            "main",
            SetBranchProtectionRequest {
                required_status_checks: vec!["ci/fast".to_string()],
                required_approving_review_count: 1,
                enforce_admins: true,
                required_linear_history: true,
                allow_force_pushes: false,
                allow_deletions: false,
                require_signed_commits: false,
                require_jankurai_proof: false,
            },
        )
        .unwrap();
        let pr = core
            .create_pull_request(
                "alice",
                "jeryu",
                "alice",
                CreatePullRequestRequest {
                    title: "change".to_string(),
                    body: None,
                    head: "feature".to_string(),
                    base: "main".to_string(),
                    head_sha: Some("abc".to_string()),
                    base_sha: None,
                    draft: false,
                    commits: Vec::new(),
                    changed_files: Vec::new(),
                },
            )
            .unwrap();
        assert!(!pr.mergeable);
        assert!(
            core.merge_pull_request(
                "alice",
                "jeryu",
                pr.number,
                MergePullRequestRequest::default()
            )
            .is_err()
        );
        core.create_review(
            "alice",
            "jeryu",
            pr.number,
            "alice",
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: Vec::new(),
            },
        )
        .unwrap();
        core.create_commit_status(
            "alice",
            "jeryu",
            "abc",
            "alice",
            CreateCommitStatusRequest {
                state: CommitStatusState::Success,
                context: "ci/fast".to_string(),
                description: None,
                target_url: None,
            },
        )
        .unwrap();
        let merge = core
            .merge_pull_request(
                "alice",
                "jeryu",
                pr.number,
                MergePullRequestRequest::default(),
            )
            .unwrap();
        assert!(merge.merged);
    }

    #[test]
    fn check_run_satisfies_required_context() {
        let core = core_with_repo();
        core.set_branch_protection(
            "alice",
            "jeryu",
            "main",
            SetBranchProtectionRequest {
                required_status_checks: vec!["ci/fast".to_string()],
                required_approving_review_count: 0,
                enforce_admins: false,
                required_linear_history: false,
                allow_force_pushes: false,
                allow_deletions: false,
                require_signed_commits: false,
                require_jankurai_proof: false,
            },
        )
        .unwrap();
        let pr = core
            .create_pull_request(
                "alice",
                "jeryu",
                "alice",
                CreatePullRequestRequest {
                    title: "change".to_string(),
                    body: None,
                    head: "feature".to_string(),
                    base: "main".to_string(),
                    head_sha: Some("abc".to_string()),
                    base_sha: None,
                    draft: false,
                    commits: Vec::new(),
                    changed_files: Vec::new(),
                },
            )
            .unwrap();
        assert!(!pr.mergeable);
        core.create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci/fast".to_string(),
                head_sha: "abc".to_string(),
                status: Some(CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                details_url: None,
                output: None,
            },
        )
        .unwrap();
        let pr = core.get_pull_request("alice", "jeryu", pr.number).unwrap();
        assert!(pr.mergeable);
    }

    #[test]
    fn webhook_outbox_records_matching_events() {
        let core = core_with_repo();
        core.create_webhook(
            "alice",
            "jeryu",
            CreateWebhookRequest {
                name: "web".to_string(),
                active: true,
                events: vec!["issues".to_string()],
                config: WebhookConfig {
                    url: "https://hooks.invalid/jeryu".to_string(),
                    content_type: "json".to_string(),
                    secret: Some("secret".to_string()),
                },
            },
        )
        .unwrap();
        core.create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "bug".to_string(),
                body: None,
                labels: Vec::new(),
                assignees: Vec::new(),
                milestone: None,
            },
        )
        .unwrap();
        let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].event, "issues");
        assert!(deliveries[0].signature_256.is_some());
    }
}
