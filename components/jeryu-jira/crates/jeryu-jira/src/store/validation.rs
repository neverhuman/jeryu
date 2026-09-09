use crate::contracts::{
    WorkFilter, WorkIssueLink, WorkItem, WorkPrincipal, WorkPrincipalKind, WorkPullRequestLink,
};
use crate::{Result, WorkError};

pub(super) fn validate_title(title: &str) -> Result<()> {
    validate_body(title, "work title")
}

pub(super) fn validate_body(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(WorkError::Validation(format!("{field} must not be empty")));
    }
    Ok(())
}

pub(super) fn validate_principal(principal: &WorkPrincipal) -> Result<()> {
    validate_body(&principal.id, "principal id")
}

pub(super) fn validate_issue_link(link: &WorkIssueLink) -> Result<()> {
    validate_body(&link.owner, "issue owner")?;
    validate_body(&link.repo, "issue repo")?;
    if link.number == 0 {
        return Err(WorkError::Validation(
            "issue number must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_pull_request_link(link: &WorkPullRequestLink) -> Result<()> {
    validate_body(&link.owner, "pull request owner")?;
    validate_body(&link.repo, "pull request repo")?;
    if link.number == 0 {
        return Err(WorkError::Validation(
            "pull request number must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn normalize_body(body: Option<String>) -> Option<String> {
    body.map(|body| body.trim().to_string())
        .filter(|body| !body.is_empty())
}

pub(super) fn normalize_labels(labels: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = labels
        .into_iter()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

pub(super) fn normalize_assignees(assignees: Vec<WorkPrincipal>) -> Result<Vec<WorkPrincipal>> {
    let mut out = Vec::new();
    for mut assignee in assignees {
        assignee.id = assignee.id.trim().to_string();
        validate_principal(&assignee)?;
        if !out
            .iter()
            .any(|held: &WorkPrincipal| held.kind == assignee.kind && held.id == assignee.id)
        {
            out.push(assignee);
        }
    }
    Ok(out)
}

pub(super) fn local_operator() -> WorkPrincipal {
    WorkPrincipal {
        kind: WorkPrincipalKind::Human,
        id: "local".to_string(),
        display_name: Some("Local Operator".to_string()),
    }
}

pub(super) fn filter_matches(item: &WorkItem, filter: &WorkFilter) -> bool {
    if let Some(repo_id) = &filter.repo_id
        && item.repo.as_ref().map(|repo| &repo.id) != Some(repo_id)
    {
        return false;
    }
    if filter.status.is_some_and(|status| item.status != status) {
        return false;
    }
    if filter.kind.is_some_and(|kind| item.kind != kind) {
        return false;
    }
    if filter
        .priority
        .is_some_and(|priority| item.priority != priority)
    {
        return false;
    }
    if let Some(assignee) = &filter.assignee
        && !item
            .assignees
            .iter()
            .any(|principal| &principal.id == assignee)
    {
        return false;
    }
    if let Some(label) = &filter.label
        && !item.labels.iter().any(|held| held == label)
    {
        return false;
    }
    if let Some(search) = filter.search.as_ref().map(|s| s.to_ascii_lowercase())
        && !search.is_empty()
    {
        let haystack = format!(
            "{} {} {}",
            item.key,
            item.title,
            item.body.as_deref().unwrap_or_default()
        )
        .to_ascii_lowercase();
        if !haystack.contains(&search) {
            return false;
        }
    }
    true
}
