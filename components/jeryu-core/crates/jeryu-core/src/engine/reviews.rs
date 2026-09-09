//! Pull request reviews and review comments.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{ForgeCore, apply_evaluation, emit_event_locked, evaluate_locked};
use crate::errors::{ForgeError, Result};
use crate::model::*;
use crate::webhooks::event_payload;

impl ForgeCore {
    pub fn create_review(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        request: CreateReviewRequest,
    ) -> Result<Review> {
        self.ensure_user(author);
        let mut state = self.state.write();
        let previous = state.clone();
        let key = (owner.to_string(), repo.to_string(), number);
        let head_sha = match state.pulls.get(&key) {
            Some(pr) => {
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
    }

    pub fn list_reviews(&self, owner: &str, repo: &str, number: u64) -> Result<Vec<Review>> {
        self.get_pull_request(owner, repo, number)?;
        // The PR exists (checked above); a missing reviews entry just means it
        // has no reviews yet, so an empty list is the intended value.
        Ok(
            match self
                .state
                .read()
                .reviews
                .get(&(owner.to_string(), repo.to_string(), number))
            {
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
            match self.state.read().review_comments.get(&(
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
