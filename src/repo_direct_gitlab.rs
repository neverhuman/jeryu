use anyhow::{Context, Result};
use std::path::Path;

use super::run_git;

pub(crate) fn local_gitlab_ssh_url(namespace: &str, name: &str) -> String {
    format!(
        "ssh://git@127.0.0.1:2224/{}/{}.git",
        namespace.trim_matches('/'),
        name.trim_end_matches(".git")
    )
}

pub(crate) async fn local_gitlab_reachable() -> bool {
    let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:8929", None);
    client.is_ready().await
}

pub(crate) async fn ensure_local_gitlab_project(
    namespace: &str,
    name: &str,
    initialize_with_readme: bool,
) -> Result<crate::gitlab_client::Project> {
    let auth = crate::gitlab_auth::resolve_or_repair("http://127.0.0.1:8929").await?;
    let client = crate::gitlab_client::GitlabClient::new(&auth.url, Some(auth.token));
    let expected = format!(
        "{}/{}",
        namespace.trim_matches('/'),
        name.trim_end_matches(".git")
    );
    let projects = client.list_projects().await?;
    if let Some(project) = projects
        .into_iter()
        .find(|project| project.path_with_namespace == expected || project.name == name)
    {
        return Ok(project);
    }
    client
        .create_project_with_readme(name, initialize_with_readme)
        .await
}

pub(crate) fn seed_missing_local_branch(
    repo_root: &Path,
    remote_url: &str,
    branch: &str,
) -> Result<()> {
    if remote_branch_exists(repo_root, remote_url, branch)? {
        return Ok(());
    }

    let Some(source_ref) = seed_source_ref(repo_root, branch)? else {
        return Ok(());
    };

    run_git(
        repo_root,
        &[
            "push",
            "--no-verify",
            remote_url,
            &format!("{source_ref}:refs/heads/{branch}"),
        ],
    )
    .with_context(|| {
        format!("seeding local GitLab branch {branch} from {source_ref} before protection")
    })
}

fn remote_branch_exists(repo_root: &Path, remote_url: &str, branch: &str) -> Result<bool> {
    let output = super::git_output(
        repo_root,
        &[
            "ls-remote",
            "--exit-code",
            "--heads",
            remote_url,
            &format!("refs/heads/{branch}"),
        ],
    )
    .with_context(|| format!("checking remote branch {branch} at {remote_url}"))?;
    Ok(output.status.success())
}

pub(crate) fn seed_source_ref(repo_root: &Path, branch: &str) -> Result<Option<String>> {
    for candidate in [
        format!("refs/heads/{branch}"),
        format!("refs/remotes/origin/{branch}"),
        "HEAD".to_string(),
    ] {
        if git_ref_exists(repo_root, &candidate)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn git_ref_exists(repo_root: &Path, ref_name: &str) -> Result<bool> {
    let output = super::git_output(
        repo_root,
        &["rev-parse", "--verify", &format!("{ref_name}^{{commit}}")],
    )
    .with_context(|| format!("checking local ref {ref_name}"))?;
    Ok(output.status.success())
}
