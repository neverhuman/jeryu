//! Adapters for `jeryu forge {repo,pr,issue}`.

use std::io::Write;

use crate::cli::{ForgeCommands, IssueCommands, PrCommands, RepoCommands};
use crate::client::{
    ClientResult, CreateIssueRequest, CreateRepositoryRequest, ForgeClient, OpenPullRequestRequest,
};
use crate::commands::render;

pub(crate) fn run(
    client: &dyn ForgeClient,
    owner: &str,
    json: bool,
    cmd: ForgeCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        ForgeCommands::Repo(repo) => run_repo(client, owner, json, repo, out),
        ForgeCommands::Pr(pr) => run_pr(client, owner, json, pr, out),
        ForgeCommands::Issue(issue) => run_issue(client, owner, json, issue, out),
    }
}

fn run_repo(
    client: &dyn ForgeClient,
    owner: &str,
    json: bool,
    cmd: RepoCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
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
    owner: &str,
    json: bool,
    cmd: PrCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
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
            // trust_tier is the risk-gate input; the stub admits all tiers.
            let _ = trust_tier;
            let outcome = client.merge_pull_request(owner, &repo, pr)?;
            render(out, json, &outcome, &outcome.message)
        }
    }
}

fn run_issue(
    client: &dyn ForgeClient,
    owner: &str,
    json: bool,
    cmd: IssueCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
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
