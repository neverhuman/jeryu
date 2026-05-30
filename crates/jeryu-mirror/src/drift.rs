use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirrorDriftReport {
    pub source_digest: String,
    pub target_digest: String,
    pub missing_in_target: Vec<String>,
    pub extra_in_target: Vec<String>,
    pub repository_drifts: Vec<RepositoryDrift>,
    pub drift_detected: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryDrift {
    pub repository: String,
    pub issue_delta: isize,
    pub pull_request_delta: isize,
    pub merge_request_delta: isize,
    pub release_delta: isize,
    pub artifact_delta: isize,
    pub webhook_delta: isize,
    pub protected_branch_delta: isize,
}

pub fn compare_archives(source: &Archive, target: &Archive) -> MirrorDriftReport {
    let source_map = repo_map(source);
    let target_map = repo_map(target);
    let source_names: BTreeSet<_> = source_map.keys().cloned().collect();
    let target_names: BTreeSet<_> = target_map.keys().cloned().collect();
    let mut report = MirrorDriftReport {
        source_digest: source.canonical_digest(),
        target_digest: target.canonical_digest(),
        missing_in_target: source_names.difference(&target_names).cloned().collect(),
        extra_in_target: target_names.difference(&source_names).cloned().collect(),
        repository_drifts: Vec::new(),
        drift_detected: false,
    };

    for name in source_names.intersection(&target_names) {
        let left = source_map
            .get(name)
            .expect("source name came from source map");
        let right = target_map
            .get(name)
            .expect("target name came from target map");
        let drift = RepositoryDrift {
            repository: name.clone(),
            issue_delta: right.issues.len() as isize - left.issues.len() as isize,
            pull_request_delta: right.pull_requests.len() as isize
                - left.pull_requests.len() as isize,
            merge_request_delta: right.merge_requests.len() as isize
                - left.merge_requests.len() as isize,
            release_delta: right.releases.len() as isize - left.releases.len() as isize,
            artifact_delta: right.artifacts.len() as isize - left.artifacts.len() as isize,
            webhook_delta: right.webhooks.len() as isize - left.webhooks.len() as isize,
            protected_branch_delta: right.protected_branches.len() as isize
                - left.protected_branches.len() as isize,
        };
        if drift.issue_delta != 0
            || drift.pull_request_delta != 0
            || drift.merge_request_delta != 0
            || drift.release_delta != 0
            || drift.artifact_delta != 0
            || drift.webhook_delta != 0
            || drift.protected_branch_delta != 0
        {
            report.repository_drifts.push(drift);
        }
    }
    report.drift_detected = !report.missing_in_target.is_empty()
        || !report.extra_in_target.is_empty()
        || !report.repository_drifts.is_empty();
    report
}

fn repo_map(archive: &Archive) -> BTreeMap<String, &RepositoryArchive> {
    archive
        .repositories
        .iter()
        .map(|repo| (repo.full_name(), repo))
        .collect()
}
