//! Jankurai audit-score recording and lookup.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{ForgeCore, require_name};
use crate::errors::{ForgeError, Result};
use crate::model::*;

/// Retained score records per (repo, branch). Every mutation rewrites the
/// whole store to SQLite, so unbounded audit history would grow that rewrite
/// without bound; the badge only ever needs the newest record.
const MAX_SCORES_PER_BRANCH: usize = 200;

impl ForgeCore {
    /// Record one audit outcome. Idempotent per `(branch, commit_sha)`: a
    /// re-ingest of the same commit replaces the prior record instead of
    /// growing history, so CI retries and backfill re-runs are safe.
    pub fn record_jankurai_score(
        &self,
        owner: &str,
        repo: &str,
        request: RecordJankuraiScoreRequest,
    ) -> Result<JankuraiScore> {
        require_name("branch", &request.branch)?;
        require_name("commit sha", &request.commit_sha)?;
        require_name("decision", &request.decision)?;
        if let Some(score) = request.score
            && score > 100
        {
            return Err(ForgeError::Validation(format!(
                "score must be 0-100, got {score}"
            )));
        }
        self.ensure_repo_exists(owner, repo)?;
        let report_json = match (&request.report, request.tool_exit) {
            (Some(report), _) => Some(report.to_string()),
            (None, Some(exit)) => Some(json!({ "tool_exit": exit }).to_string()),
            (None, None) => None,
        };
        let record = JankuraiScore {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch: request.branch.trim().to_string(),
            commit_sha: request.commit_sha.trim().to_string(),
            score: request.score,
            hard_findings: request.hard_findings.unwrap_or(0),
            decision: request.decision.trim().to_string(),
            caps_applied: request.caps_applied,
            report_json,
            created_at: Utc::now(),
        };
        let mut state = self.state.write();
        let previous = state.clone();
        let scores = state
            .jankurai_scores
            .entry((owner.to_string(), repo.to_string()))
            .or_default();
        scores.retain(|existing| {
            !(existing.branch == record.branch && existing.commit_sha == record.commit_sha)
        });
        scores.push(record.clone());
        while scores
            .iter()
            .filter(|existing| existing.branch == record.branch)
            .count()
            > MAX_SCORES_PER_BRANCH
        {
            let oldest = scores
                .iter()
                .enumerate()
                .filter(|(_, existing)| existing.branch == record.branch)
                .min_by_key(|(_, existing)| existing.created_at)
                .map(|(index, _)| index)
                .expect("count > cap implies at least one entry");
            scores.remove(oldest);
        }
        self.persist_after_mutation(&mut state, previous)?;
        Ok(record)
    }

    /// Scores for a repo, newest first, optionally filtered by branch and/or
    /// commit sha.
    pub fn list_jankurai_scores(
        &self,
        owner: &str,
        repo: &str,
        branch: Option<&str>,
        commit_sha: Option<&str>,
    ) -> Result<Vec<JankuraiScore>> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
        let mut scores: Vec<JankuraiScore> = state
            .jankurai_scores
            .get(&(owner.to_string(), repo.to_string()))
            .map(|scores| {
                scores
                    .iter()
                    .filter(|score| branch.is_none_or(|branch| score.branch == branch))
                    .filter(|score| commit_sha.is_none_or(|sha| score.commit_sha == sha))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        scores.sort_by_key(|score| std::cmp::Reverse(score.created_at));
        Ok(scores)
    }

    /// Newest score for a branch, if any.
    pub fn latest_jankurai_score(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Option<JankuraiScore> {
        self.state
            .read()
            .jankurai_scores
            .get(&(owner.to_string(), repo.to_string()))
            .and_then(|scores| {
                scores
                    .iter()
                    .filter(|score| score.branch == branch)
                    .max_by_key(|score| score.created_at)
                    .cloned()
            })
    }
}
