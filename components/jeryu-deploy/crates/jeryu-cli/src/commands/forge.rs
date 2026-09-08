//! Adapters for `jeryu forge {repo,pr,issue}`.

use std::io::Write;

use crate::cli::{ForgeCommands, IssueCommands, PrCommands, RepoCommands};
use crate::client::{
    ClientError, ClientResult, CreateIssueRequest, CreateRepositoryRequest, ForgeClient, Issue,
    IssueState, MergeOutcome, OpenPullRequestRequest, PullRequest, PullRequestState, Repository,
};
use crate::commands::api::ApiClient;
use crate::commands::render;
use serde_json::{Value, json};

pub(crate) fn run(
    client: &dyn ForgeClient,
    api_url: Option<&str>,
    owner: &str,
    json: bool,
    cmd: ForgeCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        ForgeCommands::Repo(repo) => run_repo(client, api_url, owner, json, repo, out),
        ForgeCommands::Pr(pr) => run_pr(client, api_url, owner, json, pr, out),
        ForgeCommands::Issue(issue) => run_issue(client, api_url, owner, json, issue, out),
    }
}

fn run_repo(
    client: &dyn ForgeClient,
    api_url: Option<&str>,
    owner: &str,
    json: bool,
    cmd: RepoCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    if let Some(api_url) = api_url {
        return run_repo_live(api_url, owner, json, cmd, out);
    }
    match cmd {
        RepoCommands::Create {
            name,
            private,
            default_branch,
        } => {
            let repo = client.create_repository(
                owner,
                CreateRepositoryRequest {
                    name,
                    private,
                    default_branch: Some(default_branch),
                },
            )?;
            render(
                out,
                json,
                &repo,
                &format!("created {}/{}", repo.owner, repo.name),
            )
        }
        RepoCommands::List => {
            let repos = client.list_repositories(Some(owner))?;
            let human = repos
                .iter()
                .map(|r| format!("{}/{}", r.owner, r.name))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &repos, &human)
        }
    }
}

fn run_pr(
    client: &dyn ForgeClient,
    api_url: Option<&str>,
    owner: &str,
    json: bool,
    cmd: PrCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    if let Some(api_url) = api_url {
        return run_pr_live(api_url, owner, json, cmd, out);
    }
    match cmd {
        PrCommands::Open {
            repo,
            head,
            base,
            title,
            draft,
        } => {
            let pr = client.open_pull_request(
                owner,
                &repo,
                OpenPullRequestRequest {
                    head,
                    base,
                    title,
                    draft,
                },
            )?;
            render(
                out,
                json,
                &pr,
                &format!(
                    "opened pull request #{} ({} -> {})",
                    pr.number, pr.head, pr.base
                ),
            )
        }
        PrCommands::List { repo } => {
            let prs = client.list_pull_requests(owner, &repo)?;
            let human = prs
                .iter()
                .map(|p| format!("#{} {} [{:?}]", p.number, p.title, p.state))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &prs, &human)
        }
        PrCommands::Status { repo, pr } => {
            let pull = client.get_pull_request(owner, &repo, pr)?;
            render(
                out,
                json,
                &pull,
                &format!("pull request #{} is {:?}", pull.number, pull.state),
            )
        }
        PrCommands::Merge {
            repo,
            pr,
            trust_tier,
        } => {
            // trust_tier is the risk-gate input; the in-memory client admits
            // all tiers.
            let _ = trust_tier;
            let outcome = client.merge_pull_request(owner, &repo, pr)?;
            render(out, json, &outcome, &outcome.message)
        }
    }
}

fn run_issue(
    client: &dyn ForgeClient,
    api_url: Option<&str>,
    owner: &str,
    json: bool,
    cmd: IssueCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    if let Some(api_url) = api_url {
        return run_issue_live(api_url, owner, json, cmd, out);
    }
    match cmd {
        IssueCommands::Create { repo, title, body } => {
            let issue = client.create_issue(owner, &repo, CreateIssueRequest { title, body })?;
            render(
                out,
                json,
                &issue,
                &format!("created issue #{}: {}", issue.number, issue.title),
            )
        }
        IssueCommands::List { repo } => {
            let issues = client.list_issues(owner, &repo)?;
            let human = issues
                .iter()
                .map(|i| format!("#{} {} [{:?}]", i.number, i.title, i.state))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &issues, &human)
        }
    }
}

fn run_repo_live(
    api_url: &str,
    owner: &str,
    json: bool,
    cmd: RepoCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let api = ApiClient::new(api_url)?;
    match cmd {
        RepoCommands::Create {
            name,
            private,
            default_branch,
        } => {
            let value = api.post(
                "/repos",
                json!({
                    "owner": owner,
                    "name": name,
                    "private": private,
                    "default_branch": default_branch,
                }),
            )?;
            let repo = repository_from_value(&value)?;
            render(
                out,
                json,
                &repo,
                &format!("created {}/{}", repo.owner, repo.name),
            )
        }
        RepoCommands::List => {
            let value = api.get("/repos")?;
            let repos = value
                .as_array()
                .ok_or_else(|| ClientError::Invalid("expected repository array".to_string()))?
                .iter()
                .map(repository_from_value)
                .collect::<ClientResult<Vec<_>>>()?
                .into_iter()
                .filter(|repo| repo.owner == owner)
                .collect::<Vec<_>>();
            let human = repos
                .iter()
                .map(|r| format!("{}/{}", r.owner, r.name))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &repos, &human)
        }
    }
}

fn run_pr_live(
    api_url: &str,
    owner: &str,
    json: bool,
    cmd: PrCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let api = ApiClient::new(api_url)?;
    match cmd {
        PrCommands::Open {
            repo,
            head,
            base,
            title,
            draft,
        } => {
            let value = api.post(
                &format!("/repos/{owner}/{repo}/pulls"),
                json!({
                    "head": head,
                    "base": base,
                    "title": title,
                    "draft": draft,
                }),
            )?;
            let pr = pull_request_from_value(&value)?;
            render(
                out,
                json,
                &pr,
                &format!(
                    "opened pull request #{} ({} -> {})",
                    pr.number, pr.head, pr.base
                ),
            )
        }
        PrCommands::List { repo } => {
            let value = api.get(&format!("/repos/{owner}/{repo}/pulls"))?;
            let prs = values_to_pull_requests(&value)?;
            let human = prs
                .iter()
                .map(|p| format!("#{} {} [{:?}]", p.number, p.title, p.state))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &prs, &human)
        }
        PrCommands::Status { repo, pr } => {
            let value = api.get(&format!("/api/v3/repos/{owner}/{repo}/pulls/{pr}"))?;
            let pull = pull_request_from_value(&value)?;
            render(
                out,
                json,
                &pull,
                &format!("pull request #{} is {:?}", pull.number, pull.state),
            )
        }
        PrCommands::Merge {
            repo,
            pr,
            trust_tier,
        } => {
            let _ = trust_tier;
            let value = api.put(
                &format!("/repos/{owner}/{repo}/pulls/{pr}/merge"),
                json!({}),
            )?;
            let merged = merge_outcome_from_value(pr, &value);
            render(out, json, &merged, &merged.message)
        }
    }
}

fn run_issue_live(
    api_url: &str,
    owner: &str,
    json: bool,
    cmd: IssueCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let api = ApiClient::new(api_url)?;
    match cmd {
        IssueCommands::Create { repo, title, body } => {
            let value = api.post(
                &format!("/repos/{owner}/{repo}/issues"),
                json!({
                    "title": title,
                    "body": body,
                }),
            )?;
            let issue = issue_from_value(&value)?;
            render(
                out,
                json,
                &issue,
                &format!("created issue #{}: {}", issue.number, issue.title),
            )
        }
        IssueCommands::List { repo } => {
            let value = api.get(&format!("/repos/{owner}/{repo}/issues"))?;
            let issues = values_to_issues(&value)?;
            let human = issues
                .iter()
                .map(|i| format!("#{} {} [{:?}]", i.number, i.title, i.state))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &issues, &human)
        }
    }
}

fn repository_from_value(value: &Value) -> ClientResult<Repository> {
    let full_name = value
        .get("full_name")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            let owner = value.get("owner")?.get("login")?.as_str()?;
            let name = value.get("name")?.as_str()?;
            Some(format!("{owner}/{name}"))
        })
        .ok_or_else(|| ClientError::Invalid("repository full_name is missing".to_string()))?;
    let (owner, name) = full_name
        .split_once('/')
        .ok_or_else(|| ClientError::Invalid("repository full_name is malformed".to_string()))?;
    Ok(Repository {
        owner: owner.to_string(),
        name: name.to_string(),
        private: value
            .get("private")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        default_branch: value
            .get("default_branch")
            .and_then(Value::as_str)
            .unwrap_or("main")
            .to_string(),
    })
}

fn issue_from_value(value: &Value) -> ClientResult<Issue> {
    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| ClientError::Invalid("issue number is missing".to_string()))?;
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| ClientError::Invalid("issue title is missing".to_string()))?;
    Ok(Issue {
        number,
        title: title.to_string(),
        state: issue_state_from_value(value),
    })
}

fn values_to_issues(value: &Value) -> ClientResult<Vec<Issue>> {
    let array = value
        .as_array()
        .ok_or_else(|| ClientError::Invalid("expected issue array".to_string()))?;
    array.iter().map(issue_from_value).collect()
}

fn pull_request_from_value(value: &Value) -> ClientResult<PullRequest> {
    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| ClientError::Invalid("pull request number is missing".to_string()))?;
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| ClientError::Invalid("pull request title is missing".to_string()))?;
    let head = value
        .get("head")
        .and_then(|head| head.get("ref"))
        .and_then(Value::as_str)
        .ok_or_else(|| ClientError::Invalid("pull request head is missing".to_string()))?;
    let base = value
        .get("base")
        .and_then(|base| base.get("ref"))
        .and_then(Value::as_str)
        .ok_or_else(|| ClientError::Invalid("pull request base is missing".to_string()))?;
    Ok(PullRequest {
        number,
        head: head.to_string(),
        base: base.to_string(),
        title: title.to_string(),
        draft: value.get("draft").and_then(Value::as_bool).unwrap_or(false),
        state: pull_request_state_from_value(value),
    })
}

fn values_to_pull_requests(value: &Value) -> ClientResult<Vec<PullRequest>> {
    let array = value
        .as_array()
        .ok_or_else(|| ClientError::Invalid("expected pull request array".to_string()))?;
    array.iter().map(pull_request_from_value).collect()
}

fn merge_outcome_from_value(number: u64, value: &Value) -> MergeOutcome {
    MergeOutcome {
        number,
        merged: value
            .get("merged")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("merge completed")
            .to_string(),
    }
}

fn issue_state_from_value(value: &Value) -> IssueState {
    match value.get("state").and_then(Value::as_str) {
        Some("closed") => IssueState::Closed,
        _ => IssueState::Open,
    }
}

fn pull_request_state_from_value(value: &Value) -> PullRequestState {
    match (
        value.get("merged").and_then(Value::as_bool),
        value.get("state").and_then(Value::as_str),
    ) {
        (Some(true), _) => PullRequestState::Merged,
        (_, Some("closed")) => PullRequestState::Closed,
        _ => PullRequestState::Open,
    }
}
