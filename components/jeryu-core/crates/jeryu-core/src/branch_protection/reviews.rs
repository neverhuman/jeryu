//! Current-head review selection shared by merge protection and readback.

use std::collections::BTreeMap;

use crate::model::{PullRequest, Review, ReviewState};

/// Return at most one effective review per reviewer for `head_sha`.
///
/// Historical reviews without a recorded head and reviews recorded at another
/// head are audit history only. Comments preserve explicit verdicts. A targeted
/// dismissal clears its current verdict without recovering an earlier one.
/// Core appends reviews to this slice, and SQLite restores it in row order.
pub fn effective_reviews_for_head<'a>(reviews: &'a [Review], head_sha: &str) -> Vec<&'a Review> {
    reduce_reviews(reviews, head_sha, None)
}

fn reduce_reviews<'a>(
    reviews: &'a [Review],
    head_sha: &str,
    pr_author: Option<&str>,
) -> Vec<&'a Review> {
    let mut latest = BTreeMap::<&str, &Review>::new();
    for review in reviews
        .iter()
        .filter(|review| review.head_sha.as_deref() == Some(head_sha))
    {
        // An invalid inherited self approval cannot erase an earlier rejection.
        if review.state == ReviewState::Approved
            && pr_author.is_some_and(|author| review.author.eq_ignore_ascii_case(author))
        {
            continue;
        }
        match review.state {
            ReviewState::Approved | ReviewState::ChangesRequested => {
                latest.insert(review.author.as_str(), review);
            }
            ReviewState::Commented => {}
            ReviewState::Dismissed => {
                if let Some(current) = latest.get(review.author.as_str()) {
                    let clears = match review.dismissed_review_id {
                        Some(target) => target == current.id,
                        // A historical unbound dismissal cannot establish
                        // approval or erase a rejection. Never infer its target.
                        None => current.state == ReviewState::Approved,
                    };
                    if clears {
                        latest.remove(review.author.as_str());
                    }
                }
            }
        }
    }
    latest.into_values().collect()
}

/// Qualification and readback use the same verdicts, excluding inherited
/// self approvals even when they were written before Core enforced the rule.
pub fn effective_reviews_for_pull_request<'a>(
    reviews: &'a [Review],
    pr: &PullRequest,
) -> Vec<&'a Review> {
    reduce_reviews(reviews, &pr.head.sha, Some(&pr.author))
}
