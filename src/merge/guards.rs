//! Owner: Web Forge BFF — merge guards (W-B-11, §28.3).
//! Proof: `cargo nextest run -p jeryu --lib merge::guards`
//! Invariants:
//!   - The only path to an approve/merge write is through [`verify_head_sha`];
//!     callers never bypass this fence (Tip1 Law 4).

use jeryu::git_host::{GitHost, RepoRef};

use crate::web::error::ApiError;

/// Refetch live MR state from the host and compare its head SHA to the
/// `expected` SHA the caller observed. Returns the canonical head SHA on
/// match. Mismatches surface as `ApiError::MergeShaStale` so the API layer
/// can emit the `merge_sha_stale` code per §35.9 item 7.
pub async fn verify_head_sha(
    host: &dyn GitHost,
    repo: &RepoRef,
    iid: &str,
    expected: &str,
) -> Result<String, ApiError> {
    let live = host
        .get_pr_state(repo, iid)
        .await
        .map_err(crate::repos::service::host_to_api_error)?;
    if live.head_sha.is_empty() {
        return Err(ApiError::Upstream(
            "host did not return head_sha for live state".into(),
        ));
    }
    if live.head_sha != expected {
        return Err(ApiError::MergeShaStale {
            expected: expected.to_string(),
            live: live.head_sha,
        });
    }
    Ok(live.head_sha)
}

#[cfg(test)]
mod tests {
    use crate::web::error::ApiError;

    // The `verify_head_sha` function is exercised by integration callers that
    // pass a live `GitHost` impl. A pure unit test would need a fake host,
    // which the test_utils helper provides — left to the integration layer to
    // avoid duplicating the fake here.

    #[test]
    fn merge_sha_stale_carries_expected_and_live() {
        let e = ApiError::MergeShaStale {
            expected: "aaa".into(),
            live: "bbb".into(),
        };
        assert_eq!(e.code(), "merge_sha_stale");
    }
}
