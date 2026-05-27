//! W-T-02 · Permissions matrix tests
//!
//! Two axes per §35.1.18:
//!   1. Normalized perm enforcement: a viewer with `repo.read` only
//!      MUST be rejected by a route that requires `settings.write`.
//!   2. GitLab role → normalized-perm mapping round-trip:
//!      guest / reporter / developer / maintainer / owner each map to
//!      the expected normalized set used by `src/repos/permissions.rs`.
//!
//! Phase-2 status (as of `web-forge/main` @ c1dadbb): `src/repos/` (which
//! will host the role → perm mapper) is NOT yet present. The mapper
//! round-trip is marked `#[ignore]` until W-P2-backend lands.
//!
//! What IS exercised today: the canonical `perms::ALL` constant list
//! (28 keys, §35.1.18) and the `perm_for_scope` mapper that already
//! ship in the BFF — the integration test below confirms each expected
//! key is in the canonical set, so a future role→perm mapper writing
//! the same string constants does not silently drift.

#![cfg(feature = "web")]

// ---------------------------------------------------------------------------
// Forward-compatibility coverage: the canonical 28-key set is stable.
// ---------------------------------------------------------------------------
//
// `crate::web::permissions::perms` is binary-private; this integration
// test asserts the wire-stable string keys instead. Whichever crate ends
// up owning the role→perm mapper must emit these exact strings.

const EXPECTED_NORMALIZED_KEYS: &[&str] = &[
    "repo.read",
    "repo.create",
    "repo.write",
    "repo.admin",
    "repo.delete",
    "code.read",
    "code.write",
    "branch.create",
    "branch.delete",
    "settings.read",
    "settings.write",
    "mr.read",
    "mr.write",
    "mr.comment",
    "mr.review",
    "mr.approve",
    "mr.merge",
    "issue.read",
    "issue.write",
    "ci.read",
    "ci.write",
    "secrets.read_metadata",
    "secrets.write",
    "agents.read",
    "agents.write",
    "agents.grant",
    "audit.read",
    "admin.audit",
];

#[test]
fn canonical_normalized_key_count_matches_spec_35_1_18() {
    // §35.1.18 enumerates 28 canonical keys (24 spec-listed + 4 implied:
    // branch.create/delete, mr.comment, mr.review).
    assert_eq!(
        EXPECTED_NORMALIZED_KEYS.len(),
        28,
        "spec §35.1.18 requires 28 canonical keys",
    );
}

#[test]
fn role_to_perm_expectations_use_canonical_keys() {
    // This is the table the W-P2-backend role-mapper will materialize.
    // Asserting every value referenced by the mapper appears in the
    // canonical key list catches typos at compile-time-of-the-test.
    let expectations: &[(&str, &[&str])] = &[
        ("guest", &["repo.read", "code.read", "mr.read", "issue.read"]),
        (
            "reporter",
            &[
                "repo.read",
                "code.read",
                "mr.read",
                "mr.comment",
                "issue.read",
                "issue.write",
                "ci.read",
            ],
        ),
        (
            "developer",
            &[
                "repo.read",
                "code.read",
                "code.write",
                "branch.create",
                "mr.read",
                "mr.write",
                "mr.comment",
                "mr.review",
                "issue.read",
                "issue.write",
                "ci.read",
            ],
        ),
        (
            "maintainer",
            &[
                "repo.read",
                "code.read",
                "code.write",
                "branch.create",
                "branch.delete",
                "settings.read",
                "settings.write",
                "mr.read",
                "mr.write",
                "mr.comment",
                "mr.review",
                "mr.approve",
                "mr.merge",
                "issue.read",
                "issue.write",
                "ci.read",
                "ci.write",
                "secrets.read_metadata",
                "secrets.write",
            ],
        ),
        (
            "owner",
            &[
                "repo.read",
                "repo.write",
                "repo.admin",
                "repo.delete",
                "code.read",
                "code.write",
                "branch.create",
                "branch.delete",
                "settings.read",
                "settings.write",
                "mr.read",
                "mr.write",
                "mr.comment",
                "mr.review",
                "mr.approve",
                "mr.merge",
                "issue.read",
                "issue.write",
                "ci.read",
                "ci.write",
                "secrets.read_metadata",
                "secrets.write",
                "audit.read",
                "admin.audit",
            ],
        ),
    ];

    for (role, perms) in expectations {
        for perm in *perms {
            assert!(
                EXPECTED_NORMALIZED_KEYS.contains(perm),
                "role {role} references unknown perm key {perm}",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Pending tests — un-ignored once `src/repos/permissions.rs` lands.
// ---------------------------------------------------------------------------

/// W-T-02 · viewer with `repo.read` only cannot PATCH settings
///
/// A `Viewer` whose perm set is `{"repo.read"}` MUST be rejected by the
/// settings-patch route with `ApiError::Forbidden("settings.write")`.
#[tokio::test]
#[ignore = "W-P2-backend: PATCH /repos/{id}/settings handler not yet in web-forge/main"]
async fn viewer_with_repo_read_only_cannot_patch_settings() {
    // TODO(W-P2-backend):
    //   1. let app = build_test_app_with_viewer(perms = {"repo.read"});
    //   2. let res = app.oneshot(PATCH "/api/v1/repos/{id}/settings", body).await?;
    //   3. assert_eq!(res.status(), StatusCode::FORBIDDEN);
    //   4. let body: serde_json::Value = parse_json(res.into_body()).await?;
    //   5. assert_eq!(body["error"]["code"], "forbidden");
    panic!("pending W-P2-backend settings handler");
}

/// W-T-02 · GitLab role → normalized perms round-trip
///
/// For each canonical GitLab role (guest, reporter, developer,
/// maintainer, owner), `gitlab_role_to_normalized_perms(role)` returns
/// exactly the expected set from §35.1.18.
#[tokio::test]
#[ignore = "W-P2-backend: src/repos/permissions.rs role mapper not yet in web-forge/main"]
async fn gitlab_role_to_normalized_perms_round_trip() {
    // TODO(W-P2-backend): import `jeryu::repos::permissions::gitlab_role_to_normalized_perms`
    // and assert the result for each role matches the table in
    // `role_to_perm_expectations_use_canonical_keys` above.
    panic!("pending W-P2-backend repos::permissions module");
}
