use anyhow::{Context, Result};
use git2::{Cred, DiffOptions, FetchOptions, Oid, RemoteCallbacks, Repository, build::RepoBuilder};

use crate::gitlab_client::GitlabClient;

pub(super) async fn changed_paths_from_repo(
    client: &GitlabClient,
    project_id: i64,
    before: &str,
    after: &str,
) -> Result<Vec<String>> {
    let project = client.get_project(project_id).await?;
    let clone_dir = std::env::temp_dir().join(format!("jeryu-impact-{}", uuid::Uuid::new_v4()));
    let repo = clone_project(client, &project.web_url, &clone_dir)
        .with_context(|| format!("cloning project {} for impact analysis", project.web_url))?;

    let before_commit = repo.find_commit(Oid::from_str(before)?)?;
    let after_commit = repo.find_commit(Oid::from_str(after)?)?;
    let before_tree = before_commit.tree()?;
    let after_tree = after_commit.tree()?;
    let mut diff_opts = DiffOptions::new();
    let diff =
        repo.diff_tree_to_tree(Some(&before_tree), Some(&after_tree), Some(&mut diff_opts))?;

    let mut changed_paths = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path().or(delta.old_file().path()) {
                changed_paths.push(path.to_string_lossy().to_string());
            }
            true
        },
        None,
        None,
        None,
    )?;

    let _ = std::fs::remove_dir_all(&clone_dir);
    Ok(changed_paths)
}

fn clone_project(
    client: &GitlabClient,
    web_url: &str,
    clone_dir: &std::path::Path,
) -> Result<Repository> {
    let mut callbacks = RemoteCallbacks::new();
    let pat = match client.pat_value_for_clone() {
        Some(value) => value,
        None => String::new(),
    };
    callbacks.credentials(move |_url, _username, _types| Cred::userpass_plaintext("oauth2", &pat));

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    let mut builder = RepoBuilder::new();
    builder.fetch_options(fetch_options);
    builder
        .clone(&format!("{}.git", web_url), clone_dir)
        .map_err(Into::into)
}
