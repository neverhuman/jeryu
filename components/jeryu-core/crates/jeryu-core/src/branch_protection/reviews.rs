//! Current-head review selection shared by merge protection and readback.

use std::collections::BTreeMap;

use crate::model::{Review, ReviewState};

/// Return at most one effective review per reviewer for `head_sha`.
///
/// Historical reviews without a recorded head and reviews recorded at another
/// head are audit history only. Dismissed rows are not effective. Among the
/// remaining rows, the latest audit row wins for each reviewer. Core appends
/// reviews to this slice, and SQLite restores it in row order.
pub fn effective_reviews_for_head<'a>(reviews: &'a [Review], head_sha: &str) -> Vec<&'a Review> {
    let mut latest = BTreeMap::<&str, &Review>::new();
    for review in reviews.iter().filter(|review| {
        review.head_sha.as_deref() == Some(head_sha) && review.state != ReviewState::Dismissed
    }) {
        latest.insert(review.author.as_str(), review);
    }
    latest.into_values().collect()
}
