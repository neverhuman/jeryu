//! Pull request reviews and review comments.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{ForgeCore, apply_evaluation, emit_event_locked, evaluate_locked};
use crate::branch_protection::effective_reviews_for_head;
use crate::errors::{ForgeError, Result};
use crate::model::*;
use crate::webhooks::event_payload;

impl ForgeCore {
    /// Compatibility advisory history from a trusted-name caller. Even a row
    /// carrying a head SHA is not an authenticated source-review event. Use
    /// the credential/challenge API for qualified review submissions.
    pub fn create_review(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        request: CreateReviewRequest,
    ) -> Result<Review> {
        if request.event == ReviewState::Dismissed {
            return Err(ForgeError::Validation(
                "use dismiss_review with a target review, exact head and reason".to_string(),
            ));
        }
        self.record_review(owner, repo, number, author, request, None)
    }

    /// The caller supplies an actor login; this method does not authenticate a
    /// credential. The result is advisory and cannot dismiss a bound verdict.
    pub fn dismiss_review(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        actor: &str,
        request: DismissReviewRequest,
    ) -> Result<Review> {
        if request.reason.trim().is_empty() {
            return Err(ForgeError::Validation(
                "review dismissal requires a non-empty reason".to_string(),
            ));
        }
        if request.expected_head_sha.len() != 40
            || !request
                .expected_head_sha
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(ForgeError::Validation(
                "review dismissal requires a full lowercase SHA-1 head".to_string(),
            ));
        }
        self.record_review(
            owner,
            repo,
            number,
            actor,
            CreateReviewRequest {
                body: Some(request.reason),
                event: ReviewState::Dismissed,
                comments: Vec::new(),
                expected_head_sha: Some(request.expected_head_sha),
            },
            Some(request.review_id),
        )
    }

    fn record_review(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        request: CreateReviewRequest,
        dismissed_review_id: Option<Uuid>,
    ) -> Result<Review> {
        self.with_profile_mutation(owner, repo, author, || {
            super::auth::require_login(author)?;
            let mut state = self.runtime.state.write();
            let key = (owner.to_string(), repo.to_string(), number);
            let head_sha = match state.pulls.get(&key) {
                Some(pr) => {
                    if request.event == ReviewState::Approved
                        && pr.author.eq_ignore_ascii_case(author)
                    {
                        return Err(ForgeError::Forbidden(
                            "pull request authors cannot approve their own changes".to_string(),
                        ));
                    }
                    if request
                        .expected_head_sha
                        .as_deref()
                        .is_some_and(|expected| expected != pr.head.sha)
                    {
                        return Err(ForgeError::Conflict(format!(
                            "pull request {owner}/{repo}#{number} head changed"
                        )));
                    }
                    pr.head.sha.clone()
                }
                None => {
                    return Err(ForgeError::NotFound(format!(
                        "pull request {owner}/{repo}#{number}"
                    )));
                }
            };
            if let Some(target_id) = dismissed_review_id {
                let reviews = state.reviews.get(&key).map(Vec::as_slice).unwrap_or(&[]);
                let target = reviews
                    .iter()
                    .find(|review| review.id == target_id)
                    .ok_or_else(|| {
                        ForgeError::NotFound(format!(
                            "review {target_id} in {owner}/{repo}#{number}"
                        ))
                    })?;
                if target.author != author {
                    return Err(ForgeError::Forbidden(
                        "reviewers may dismiss only their own verdicts".to_string(),
                    ));
                }
                if !matches!(
                    target.state,
                    ReviewState::Approved | ReviewState::ChangesRequested
                ) {
                    return Err(ForgeError::Validation(
                        "dismissal target must be an explicit review verdict".to_string(),
                    ));
                }
                if target.head_sha.as_deref() != Some(head_sha.as_str())
                    || !effective_reviews_for_head(reviews, &head_sha)
                        .iter()
                        .any(|review| review.id == target_id)
                {
                    return Err(ForgeError::Conflict(
                        "dismissal target is not the reviewer's current-head verdict".to_string(),
                    ));
                }
            }
            let previous = state.clone();
            // Preserve the existing trusted-caller profile behavior in the same
            // transaction as the review. A profile is not an authenticated account.
            state
                .users
                .entry(author.to_string())
                .or_insert_with(|| User {
                    id: Uuid::new_v4(),
                    login: author.to_string(),
                    name: None,
                    email: None,
                    created_at: Utc::now(),
                });
            let review_id = Uuid::new_v4();
            let review = Review {
                id: review_id,
                owner: owner.to_string(),
                repo: repo.to_string(),
                pull_number: number,
                author: author.to_string(),
                state: request.event,
                body: request.body,
                head_sha: Some(head_sha),
                dismissed_review_id,
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
            self.persist_after_mutation(&mut state, previous)?;
            Ok(review)
        })
    }

    pub fn list_reviews(&self, owner: &str, repo: &str, number: u64) -> Result<Vec<Review>> {
        self.get_pull_request(owner, repo, number)?;
        // The PR exists (checked above); a missing reviews entry just means it
        // has no reviews yet, so an empty list is the intended value.
        Ok(
            match self.runtime.state.read().reviews.get(&(
                owner.to_string(),
                repo.to_string(),
                number,
            )) {
                Some(reviews) => reviews.clone(),
                None => Vec::new(),
            },
        )
    }

    pub fn list_review_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<ReviewComment>> {
        self.get_pull_request(owner, repo, number)?;
        // The PR exists (checked above); a missing review-comments entry just
        // means it has no review comments yet, so an empty list is intended.
        Ok(
            match self.runtime.state.read().review_comments.get(&(
                owner.to_string(),
                repo.to_string(),
                number,
            )) {
                Some(comments) => comments.clone(),
                None => Vec::new(),
            },
        )
    }
}
