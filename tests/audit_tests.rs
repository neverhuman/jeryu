//! W-T-02 · Audit-on-every-mutation tests
//!
//! Spec §35.1.16: every mutating endpoint MUST write exactly one row to
//! `audit_events` (and, for action-tagged calls, one row to
//! `web_action_receipts`) in the same transaction as the business
//! mutation. The parameterized table below enumerates the endpoints the
//! BFF will expose so the orchestrator can sweep them once each handler
//! lands.
//!
//! Phase-2 status (as of `web-forge/main` @ c1dadbb): no mutating
//! handlers exist on `/api/v1/*` yet (only `GET /api/v1/bootstrap`).
//! The sweep is therefore marked `#[ignore]` and exercises only the
//! audit-target string-format invariants today.

#![cfg(feature = "web")]

// ---------------------------------------------------------------------------
// Forward-compatibility: the (method, path, action_id, audit_target_format,
// risk_tier) table the audit-sweep will iterate over once the handlers
// land. Keeping this list here documents the contract and pins the
// audit-target format so future handlers can't silently drift to
// `repo/<id>` (slash) instead of `repo:<id>` (colon).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct MutatingEndpoint {
    method: &'static str,
    path: &'static str,
    action_id: &'static str,
    /// Format string for the audit `target` column. `<id>` placeholder
    /// indicates the route's path parameter.
    target_format: &'static str,
    risk_tier: &'static str,
}

const MUTATING_ENDPOINTS: &[MutatingEndpoint] = &[
    MutatingEndpoint {
        method: "POST",
        path: "/api/v1/repos",
        action_id: "repo.create",
        target_format: "repo:<id>",
        risk_tier: "medium",
    },
    MutatingEndpoint {
        method: "PATCH",
        path: "/api/v1/repos/{id}/settings",
        action_id: "settings.patch",
        target_format: "repo:<id>",
        risk_tier: "medium",
    },
    MutatingEndpoint {
        method: "POST",
        path: "/api/v1/repos/{id}/merge-requests/{iid}/approve",
        action_id: "mr.approve",
        target_format: "mr:<id>/<iid>",
        risk_tier: "high",
    },
    MutatingEndpoint {
        method: "POST",
        path: "/api/v1/repos/{id}/merge-requests/{iid}/merge",
        action_id: "mr.merge",
        target_format: "mr:<id>/<iid>",
        risk_tier: "high",
    },
    MutatingEndpoint {
        method: "POST",
        path: "/api/v1/repos/{id}/merge-requests/{iid}/comments",
        action_id: "mr.comment",
        target_format: "mr:<id>/<iid>",
        risk_tier: "low",
    },
    MutatingEndpoint {
        method: "DELETE",
        path: "/api/v1/repos/{id}",
        action_id: "repo.delete",
        target_format: "repo:<id>",
        risk_tier: "high",
    },
];

#[test]
fn mutating_endpoint_table_uses_canonical_target_format() {
    // The §35.1.16 audit invariant requires `target` strings to use a
    // colon-delimited `<entity_kind>:<id>` shape so they round-trip
    // through the bus subscribe filter (§35.1.6).
    for ep in MUTATING_ENDPOINTS {
        assert!(
            ep.target_format.contains(':'),
            "endpoint {} {} target_format must contain ':' delimiter (got {:?})",
            ep.method,
            ep.path,
            ep.target_format,
        );
        // Risk tier must be one of the three canonical labels.
        assert!(
            matches!(ep.risk_tier, "low" | "medium" | "high"),
            "endpoint {} {} risk_tier must be low/medium/high (got {:?})",
            ep.method,
            ep.path,
            ep.risk_tier,
        );
        // Action id must be lowercase snake_case (no slashes or spaces).
        assert!(
            ep.action_id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_'),
            "endpoint {} {} action_id must be canonical snake_case (got {:?})",
            ep.method,
            ep.path,
            ep.action_id,
        );
    }
}

#[test]
fn mutating_endpoint_table_covers_the_core_surface() {
    // Smoke check that the table at least lists the four §35.1.16 high-
    // risk endpoints. The sweep test below iterates this list once each
    // handler lands.
    let must_have = ["repo.create", "settings.patch", "mr.approve", "mr.merge"];
    for action in must_have {
        assert!(
            MUTATING_ENDPOINTS.iter().any(|e| e.action_id == action),
            "audit sweep table missing canonical mutation {action}",
        );
    }
}

// ---------------------------------------------------------------------------
// Pending sweep — un-ignored once each handler lands.
// ---------------------------------------------------------------------------

/// W-T-02 · audit row written for every mutating endpoint
///
/// Parameterized over `MUTATING_ENDPOINTS`. For each entry, invoke the
/// handler with a valid body and assert `audit_events` count grew by
/// exactly one with the expected `action_id`, `target`, and `risk_tier`.
#[tokio::test]
#[ignore = "W-P2-backend: mutating handlers not yet in web-forge/main; un-ignore once each lands"]
async fn every_mutating_endpoint_writes_audit_row() {
    // TODO(W-P2-backend):
    //   1. let app = build_test_app();
    //   2. for ep in MUTATING_ENDPOINTS {
    //         let before = audit_count();
    //         let res = app.oneshot(Request::builder()
    //             .method(ep.method)
    //             .uri(materialize_path(ep.path))
    //             .body(valid_body_for(ep)?)?).await?;
    //         assert!(res.status().is_success() || res.status() == StatusCode::CONFLICT);
    //         assert_eq!(audit_count(), before + 1);
    //         let row = latest_audit_row();
    //         assert_eq!(row.action_id, ep.action_id);
    //         assert_eq!(row.risk_tier, ep.risk_tier);
    //      }
    panic!("pending W-P2-backend mutating handlers");
}
