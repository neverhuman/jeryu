use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::errors::{JeryuMirrorError, Result};
use crate::model::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreOptions {
    pub dry_run: bool,
    pub require_empty_target: bool,
    pub restore_webhooks: bool,
    pub restore_app_installations: bool,
    pub restore_jeryu_artifact_metadata: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            dry_run: true,
            require_empty_target: false,
            restore_webhooks: true,
            restore_app_installations: true,
            restore_jeryu_artifact_metadata: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreReport {
    pub dry_run: bool,
    pub repositories_planned: usize,
    pub issues_planned: usize,
    pub pull_requests_planned: usize,
    pub releases_planned: usize,
    pub artifacts_planned: usize,
    pub webhooks_planned: usize,
    pub app_installations_planned: usize,
    pub protected_branches_planned: usize,
    pub secret_rehydration_required: Vec<String>,
    pub commands: Vec<String>,
    pub blockers: Vec<String>,
}

pub trait RestoreTarget {
    fn has_repository(&self, owner: &str, name: &str) -> bool;
    fn create_repository(&mut self, repo: &RepositoryArchive) -> Result<()>;
    fn create_issue(&mut self, repo: &RepositoryArchive, issue: &NormalizedIssue) -> Result<()>;
    fn create_pull_request(
        &mut self,
        repo: &RepositoryArchive,
        pr: &NormalizedPullRequest,
    ) -> Result<()>;
    fn create_release(&mut self, repo: &RepositoryArchive, release: &ReleaseArchive) -> Result<()>;
    fn record_artifact(
        &mut self,
        repo: &RepositoryArchive,
        artifact: &ArtifactMetadata,
    ) -> Result<()>;
    fn create_protected_branch(
        &mut self,
        repo: &RepositoryArchive,
        branch: &ProtectedBranchArchive,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InMemoryRestoreTarget {
    pub repositories: BTreeSet<String>,
    pub issue_count: usize,
    pub pull_request_count: usize,
    pub release_count: usize,
    pub artifact_count: usize,
    pub protected_branch_count: usize,
}

impl RestoreTarget for InMemoryRestoreTarget {
    fn has_repository(&self, owner: &str, name: &str) -> bool {
        self.repositories.contains(&format!("{owner}/{name}"))
    }

    fn create_repository(&mut self, repo: &RepositoryArchive) -> Result<()> {
        self.repositories.insert(repo.full_name());
        Ok(())
    }

    fn create_issue(&mut self, _repo: &RepositoryArchive, _issue: &NormalizedIssue) -> Result<()> {
        self.issue_count += 1;
        Ok(())
    }

    fn create_pull_request(
        &mut self,
        _repo: &RepositoryArchive,
        _pr: &NormalizedPullRequest,
    ) -> Result<()> {
        self.pull_request_count += 1;
        Ok(())
    }

    fn create_release(
        &mut self,
        _repo: &RepositoryArchive,
        _release: &ReleaseArchive,
    ) -> Result<()> {
        self.release_count += 1;
        Ok(())
    }

    fn record_artifact(
        &mut self,
        _repo: &RepositoryArchive,
        _artifact: &ArtifactMetadata,
    ) -> Result<()> {
        self.artifact_count += 1;
        Ok(())
    }

    fn create_protected_branch(
        &mut self,
        _repo: &RepositoryArchive,
        _branch: &ProtectedBranchArchive,
    ) -> Result<()> {
        self.protected_branch_count += 1;
        Ok(())
    }
}

pub fn plan_restore<T: RestoreTarget>(
    archive: &Archive,
    target: &mut T,
    options: RestoreOptions,
) -> Result<RestoreReport> {
    let mut report = RestoreReport {
        dry_run: options.dry_run,
        ..RestoreReport::default()
    };
    let mut seen = BTreeMap::new();

    for repo in &archive.repositories {
        let full_name = repo.full_name();
        if seen.insert(full_name.clone(), ()).is_some() {
            return Err(JeryuMirrorError::RestoreInvariant(format!(
                "duplicate repository {full_name}"
            )));
        }
        if options.require_empty_target && target.has_repository(&repo.owner, &repo.name) {
            report
                .blockers
                .push(format!("target already has repository {full_name}"));
            continue;
        }
        report
            .commands
            .push(format!("create repository {full_name}"));
        report.repositories_planned += 1;
        if !options.dry_run {
            target.create_repository(repo)?;
        }

        for branch in &repo.protected_branches {
            report.commands.push(format!(
                "restore branch protection {full_name}:{}",
                branch.pattern
            ));
            report.protected_branches_planned += 1;
            if !options.dry_run {
                target.create_protected_branch(repo, branch)?;
            }
        }
        for issue in &repo.issues {
            report
                .commands
                .push(format!("restore issue {full_name}#{}", issue.number));
            report.issues_planned += 1;
            if !options.dry_run {
                target.create_issue(repo, issue)?;
            }
        }
        for pr in &repo.pull_requests {
            report
                .commands
                .push(format!("restore pull request {full_name}#{}", pr.number));
            report.pull_requests_planned += 1;
            if !options.dry_run {
                target.create_pull_request(repo, pr)?;
            }
        }
        for release in &repo.releases {
            report
                .commands
                .push(format!("restore release {full_name}@{}", release.tag_name));
            report.releases_planned += 1;
            if !options.dry_run {
                target.create_release(repo, release)?;
            }
        }
        if options.restore_jeryu_artifact_metadata {
            for artifact in &repo.artifacts {
                report.commands.push(format!(
                    "record artifact metadata {full_name}:{}",
                    artifact.name
                ));
                report.artifacts_planned += 1;
                if !options.dry_run {
                    target.record_artifact(repo, artifact)?;
                }
            }
        }
        if options.restore_webhooks {
            for webhook in &repo.webhooks {
                report.commands.push(format!(
                    "restore webhook metadata {full_name}:{}",
                    webhook.url
                ));
                report.webhooks_planned += 1;
                if let Some(secret_name) = &webhook.secret_name {
                    report.secret_rehydration_required.push(secret_name.clone());
                }
            }
        }
        if options.restore_app_installations {
            for app in &repo.app_installations {
                report.commands.push(format!(
                    "restore app installation metadata {full_name}:{}",
                    app.slug
                ));
                report.app_installations_planned += 1;
                if let Some(secret_name) = &app.token_secret_name {
                    report.secret_rehydration_required.push(secret_name.clone());
                }
            }
        }
    }

    report.secret_rehydration_required.sort();
    report.secret_rehydration_required.dedup();
    Ok(report)
}
