//! W-T-02 · Global search service tests
//!
//! Spec §35.1.18 / W-B-16: fuzzy search across owner, name, and file
//! path; results ranked by score (recency × match-precision).
//!
//! Phase-2 status (as of `web-forge/main` @ c1dadbb): `RepoService::search`
//! is NOT yet present. Skeleton placeholder under `#[ignore]` lives here
//! so the W-B-16 work package can flip it on once `src/repos/search.rs`
//! lands.

#![cfg(feature = "web")]

// ---------------------------------------------------------------------------
// W-B-16 owner: search ranking + fuzzy match algorithm
// ---------------------------------------------------------------------------

/// W-T-02 · fuzzy search across owner / name / file path
///
/// `RepoService::search(query)` returns ranked results across:
///   - repo owners (slug prefix match)
///   - repo names (substring + Levenshtein within edit distance 2)
///   - file paths (BM25 over the indexed corpus)
#[tokio::test]
#[ignore = "W-B-16: RepoService::search not yet in web-forge/main; un-ignore once W-B-16 lands"]
async fn search_returns_ranked_results_across_owner_name_path() {
    // TODO(W-B-16):
    //   1. let svc = RepoService::with_seeded_index(seed::demo_repos());
    //   2. let r1 = svc.search("veox-").await?;
    //   3. assert!(r1.results.iter().any(|h| h.kind == "owner"));
    //   4. let r2 = svc.search("RedlinDB").await?; // fuzzy match
    //   5. assert!(r2.results.iter().any(|h| h.entity == "repo:redlineDB"));
    //   6. let r3 = svc.search("src/web/mod.rs").await?;
    //   7. assert!(r3.results.iter().any(|h| h.kind == "file_path"));
    //   8. assert!(r1.results.first().map(|h| h.score).unwrap_or(0.0) >= 0.5);
    panic!("pending W-B-16 RepoService::search");
}
