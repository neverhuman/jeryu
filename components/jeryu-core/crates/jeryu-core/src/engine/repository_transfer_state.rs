//! In-memory re-keying for a UUID-preserving repository transfer.

use std::collections::HashMap;
use std::hash::Hash;

use super::State;
use crate::{ForgeError, RepositoryTransferJournal, Result};

pub(super) fn rekey_repository(
    state: &mut State,
    transfer: &RepositoryTransferJournal,
) -> Result<()> {
    let old = (&transfer.source_owner, &transfer.source_name);
    let new = (&transfer.destination_owner, &transfer.destination_name);
    if state.repos.contains_key(&(new.0.clone(), new.1.clone()))
        || state
            .repository_aliases
            .contains_key(&(new.0.clone(), new.1.clone()))
    {
        return Err(ForgeError::Conflict(format!(
            "destination repository {}/{}",
            new.0, new.1
        )));
    }
    let mut repository = state
        .repos
        .remove(&(old.0.clone(), old.1.clone()))
        .ok_or_else(|| ForgeError::NotFound(format!("repository {}/{}", old.0, old.1)))?;
    if repository.id != transfer.repository_id {
        return Err(ForgeError::Validation(format!(
            "repository UUID drift for {}/{}",
            old.0, old.1
        )));
    }
    repository.owner = new.0.clone();
    repository.name = new.1.clone();
    repository.full_name = format!("{}/{}", new.0, new.1);
    repository.updated_at = chrono::Utc::now();
    state
        .repos
        .insert((new.0.clone(), new.1.clone()), repository);

    rekey_triples(&mut state.labels, old, new, |_| {});
    rekey_triples(&mut state.issues, old, new, |issue| {
        issue.owner = new.0.clone();
        issue.repo = new.1.clone();
    });
    rekey_triples(&mut state.issue_comments, old, new, |comments| {
        for comment in comments {
            comment.owner = new.0.clone();
            comment.repo = new.1.clone();
        }
    });
    rekey_triples(&mut state.pulls, old, new, |pull| {
        pull.owner = new.0.clone();
        pull.repo = new.1.clone();
        if pull.source_repository == format!("{}/{}", old.0, old.1) {
            pull.source_repository = format!("{}/{}", new.0, new.1);
        }
    });
    rekey_triples(&mut state.reviews, old, new, |reviews| {
        for review in reviews {
            review.owner = new.0.clone();
            review.repo = new.1.clone();
        }
    });
    rekey_triples(&mut state.review_comments, old, new, |comments| {
        for comment in comments {
            comment.owner = new.0.clone();
            comment.repo = new.1.clone();
        }
    });
    rekey_triples(&mut state.branch_protections, old, new, |rule| {
        rule.owner = new.0.clone();
        rule.repo = new.1.clone();
    });
    rekey_pairs(&mut state.codeowners, old, new, |_| {});
    rekey_pairs(&mut state.readmes, old, new, |_| {});
    rekey_triples(&mut state.statuses, old, new, |statuses| {
        for status in statuses {
            status.owner = new.0.clone();
            status.repo = new.1.clone();
        }
    });
    rekey_pairs(&mut state.check_runs, old, new, |runs| {
        for run in runs {
            run.owner = new.0.clone();
            run.repo = new.1.clone();
        }
    });
    rekey_pairs(&mut state.webhooks, old, new, |hooks| {
        for hook in hooks {
            hook.owner = new.0.clone();
            hook.repo = new.1.clone();
        }
    });
    rekey_pairs(&mut state.counters, old, new, |_| {});
    rekey_pairs(&mut state.jankurai_scores, old, new, |scores| {
        for score in scores {
            score.owner = new.0.clone();
            score.repo = new.1.clone();
        }
    });
    rekey_grants(state, old, new);
    for delivery in &mut state.webhook_deliveries {
        if delivery.owner == *old.0 && delivery.repo == *old.1 {
            delivery.owner = new.0.clone();
            delivery.repo = new.1.clone();
        }
    }
    Ok(())
}

fn rekey_pairs<V, F>(
    map: &mut HashMap<(String, String), V>,
    old: (&String, &String),
    new: (&String, &String),
    mut update: F,
) where
    F: FnMut(&mut V),
{
    if let Some(mut value) = map.remove(&(old.0.clone(), old.1.clone())) {
        update(&mut value);
        map.insert((new.0.clone(), new.1.clone()), value);
    }
}

fn rekey_triples<K, V, F>(
    map: &mut HashMap<(String, String, K), V>,
    old: (&String, &String),
    new: (&String, &String),
    mut update: F,
) where
    K: Clone + Eq + Hash,
    F: FnMut(&mut V),
{
    let keys: Vec<_> = map
        .keys()
        .filter(|(owner, repo, _)| owner == old.0 && repo == old.1)
        .cloned()
        .collect();
    for key in keys {
        if let Some(mut value) = map.remove(&key) {
            update(&mut value);
            map.insert((new.0.clone(), new.1.clone(), key.2), value);
        }
    }
}

fn rekey_grants(state: &mut State, old: (&String, &String), new: (&String, &String)) {
    let keys: Vec<_> = state
        .repo_grants
        .keys()
        .filter(|(_, owner, repo)| owner == old.0 && repo == old.1)
        .cloned()
        .collect();
    for key in keys {
        if let Some(mut grant) = state.repo_grants.remove(&key) {
            grant.owner = new.0.clone();
            grant.repo = new.1.clone();
            state
                .repo_grants
                .insert((key.0, new.0.clone(), new.1.clone()), grant);
        }
    }
}
