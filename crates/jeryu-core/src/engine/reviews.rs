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
        if !state
            .pulls
            .contains_key(&(owner.to_string(), repo.to_string(), number))
        {
            return Err(ForgeError::NotFound(format!(
                "pull request {owner}/{repo}#{number}"
            )));
        }
        let review_id = Uuid::new_v4();
        let review = Review {
            id: review_id,
            owner: owner.to_string(),
            repo: repo.to_string(),
            pull_number: number,
            author: author.to_string(),
            state: request.event,
            body: request.body,
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
        Ok(review)
    }

    pub fn list_reviews(&self, owner: &str, repo: &str, number: u64) -> Result<Vec<Review>> {
        self.get_pull_request(owner, repo, number)?;
        // The PR exists (checked above); a missing reviews entry just means it
        // has no reviews yet, so an empty list is the intended value.
        Ok(self
            .state
            .read()
            .reviews
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .unwrap_or_default())
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
        Ok(self
            .state
            .read()
            .review_comments
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .unwrap_or_default())
    }
}
