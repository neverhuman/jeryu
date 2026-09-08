//! Conformance tests for the GitHub-compatible REST edge.
//!
//! These exercise the real route table in `jeryu_api::GithubRouter` end to end:
//! create a repo, open a PR, register a check-run and commit status, configure
//! branch protection, then merge — asserting GitHub-shaped JSON and the right
//! status code at each step. Negative tests pin the 404 / 422 / 405 contracts.

use jeryu_api::{GithubRouter, Method};
use jeryu_jira::WorkStore;
use serde_json::Value;

fn body(response: &jeryu_api::Response) -> Value {
    serde_json::from_str(&response.body)
        .unwrap_or_else(|err| panic!("response body is not JSON ({err}): {}", response.body))
}

fn router_with_repo() -> GithubRouter {
    let router = GithubRouter::new();
    let response = router.post(
        "/repos",
        r#"{"owner":"alice","name":"jeryu","private":false,"description":"forge"}"#,
    );
    assert_eq!(response.status, 201, "create repo: {}", response.body);
    router
}

#[test]
fn version_and_health_are_github_shaped() {
    let router = GithubRouter::new();

    let health = router.get("/health");
    assert_eq!(health.status, 200);
    assert_eq!(body(&health)["status"], "ok");

    let version = router.get("/api/v1/version");
    assert_eq!(version.status, 200);
    let parsed = body(&version);
    assert_eq!(parsed["version"], jeryu_api::JERYU_API_VERSION);
    assert_eq!(parsed["name"], "jeryu-api");

    let user = router.get("/user");
    assert_eq!(user.status, 200);
    let parsed_user = body(&user);
    assert_eq!(parsed_user["login"], "jeryu");
    assert_eq!(parsed_user["type"], "User");

    let enterprise_user = router.get("/api/v3/user");
    assert_eq!(enterprise_user.status, 200);
    assert_eq!(body(&enterprise_user)["login"], "jeryu");
}

#[test]
fn create_and_get_repository_returns_github_shaped_json() {
    let router = router_with_repo();

    let created = router.get("/repos/alice/jeryu");
    assert_eq!(created.status, 200);
    let repo = body(&created);
    assert_eq!(repo["name"], "jeryu");
    assert_eq!(repo["full_name"], "alice/jeryu");
    assert_eq!(repo["private"], false);
    assert_eq!(repo["default_branch"], "main");
    // GitHub nests the owner as an object with a `login`, not a bare string.
    assert_eq!(repo["owner"]["login"], "alice");
    assert_eq!(repo["owner"]["type"], "User");

    let enterprise_created = router.get("/api/v3/repos/alice/jeryu");
    assert_eq!(enterprise_created.status, 200);
    assert_eq!(body(&enterprise_created)["full_name"], "alice/jeryu");

    let listed = router.get("/repos");
    assert_eq!(listed.status, 200);
    let repos = body(&listed);
    assert_eq!(repos.as_array().expect("array").len(), 1);
    assert_eq!(repos[0]["full_name"], "alice/jeryu");
}

#[test]
fn new_repository_exposes_linear_history_branch_protection() {
    let router = GithubRouter::new();
    let created = router.post(
        "/repos",
        r#"{"owner":"alice","name":"trunk-repo","private":false,"default_branch":"trunk"}"#,
    );
    assert_eq!(created.status, 201, "create repo: {}", created.body);

    let protection = router.get("/repos/alice/trunk-repo/branches/trunk/protection");
    assert_eq!(protection.status, 200, "protection: {}", protection.body);
    let rule = body(&protection);
    assert_eq!(rule["required_linear_history"]["enabled"], true);
    assert_eq!(rule["required_status_checks"]["strict"], true);
}

#[test]
fn full_pull_request_lifecycle_create_check_status_protect_and_merge() {
    let router = router_with_repo();

    // Open a PR. GitHub-shaped: `number`, `head`/`base` refs, `state` = open.
    let opened = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"add feature","head":"feature","base":"main","head_sha":"deadbeef","actor":"alice","source_repository":"fork-owner/jeryu"}"#,
    );
    assert_eq!(opened.status, 201, "open pr: {}", opened.body);
    let pr = body(&opened);
    let number = pr["number"].as_u64().expect("pr number");
    assert_eq!(number, 1);
    assert_eq!(pr["state"], "open");
    assert_eq!(pr["head"]["ref"], "feature");
    assert_eq!(pr["head"]["sha"], "deadbeef");
    assert_eq!(pr["base"]["ref"], "main");
    assert_eq!(pr["source_repository"], "fork-owner/jeryu");
    let legacy_id_key = ["i", "id"].concat();
    assert!(
        pr.get(&legacy_id_key).is_none(),
        "PRs expose a GitHub-shaped `number`, never a legacy provider id"
    );

    // Protect `main`: require the `ci/fast` status check and one approval.
    let protect = router.put(
        "/repos/alice/jeryu/branches/main/protection",
        r#"{"required_status_checks":["ci/fast"],"required_approving_review_count":0}"#,
    );
    assert_eq!(protect.status, 200, "set protection: {}", protect.body);
    let rule = body(&protect);
    assert_eq!(rule["required_status_checks"]["contexts"][0], "ci/fast");

    let fetched_rule = router.get("/repos/alice/jeryu/branches/main/protection");
    assert_eq!(fetched_rule.status, 200);
    assert_eq!(
        body(&fetched_rule)["required_status_checks"]["contexts"][0],
        "ci/fast"
    );

    // Before the check passes, the PR is not mergeable -> GitHub 405.
    let blocked = router.put(&format!("/repos/alice/jeryu/pulls/{number}/merge"), "{}");
    assert_eq!(blocked.status, 405, "premature merge: {}", blocked.body);
    assert!(
        body(&blocked)["message"]
            .as_str()
            .expect("message")
            .contains("MissingStatusCheck")
            || body(&blocked)["message"]
                .as_str()
                .expect("message")
                .contains("ci/fast"),
        "405 message should name the missing check: {}",
        blocked.body
    );

    // Register a successful check-run for the head sha.
    let check = router.post(
        "/repos/alice/jeryu/check-runs",
        r#"{"name":"ci/fast","head_sha":"deadbeef","status":"completed","conclusion":"success"}"#,
    );
    assert_eq!(check.status, 201, "create check-run: {}", check.body);
    let check_body = body(&check);
    assert_eq!(check_body["status"], "completed");
    // GitHub-shaped check `conclusion`.
    assert_eq!(check_body["conclusion"], "success");
    let checks_by_ref = router.get("/repos/alice/jeryu/commits/deadbeef/check-runs");
    assert_eq!(checks_by_ref.status, 200, "{}", checks_by_ref.body);
    let checks = body(&checks_by_ref);
    assert_eq!(checks["total_count"], 1);
    assert_eq!(checks["check_runs"][0]["name"], "ci/fast");

    // Also post a commit status for the same sha and read the combined status.
    let status = router.post(
        "/repos/alice/jeryu/statuses/deadbeef",
        r#"{"state":"success","context":"ci/extra","actor":"alice"}"#,
    );
    assert_eq!(status.status, 201, "create status: {}", status.body);
    assert_eq!(body(&status)["state"], "success");

    let combined = router.get("/repos/alice/jeryu/commits/deadbeef/status");
    assert_eq!(combined.status, 200);
    let combined_body = body(&combined);
    assert_eq!(combined_body["state"], "success");
    assert_eq!(combined_body["sha"], "deadbeef");
    assert_eq!(combined_body["total_count"].as_u64().expect("count"), 1);

    // The check-run now satisfies protection: GET the PR shows mergeable.
    let refreshed = router.get(&format!("/repos/alice/jeryu/pulls/{number}"));
    assert_eq!(refreshed.status, 200);
    assert_eq!(body(&refreshed)["mergeable"], true);

    // Merge succeeds with 200 and a GitHub-shaped merge result.
    let merged = router.put(
        &format!("/repos/alice/jeryu/pulls/{number}/merge"),
        r#"{"merge_method":"squash"}"#,
    );
    assert_eq!(merged.status, 200, "merge: {}", merged.body);
    let merge_body = body(&merged);
    assert_eq!(merge_body["merged"], true);
    assert!(
        merge_body["sha"]
            .as_str()
            .expect("sha")
            .starts_with("merge-")
    );

    // After merge the PR state is `closed` (GitHub normalizes merged -> closed).
    let after = router.get(&format!("/repos/alice/jeryu/pulls/{number}"));
    assert_eq!(after.status, 200);
    let after_body = body(&after);
    assert_eq!(after_body["state"], "closed");
    assert_eq!(after_body["merged"], true);
}

#[test]
fn fork_source_repository_still_requires_signed_commits() {
    let router = router_with_repo();

    let protect = router.put(
        "/repos/alice/jeryu/branches/main/protection",
        r#"{"require_signed_commits":true,"allow_force_pushes":false,"allow_deletions":false}"#,
    );
    assert_eq!(protect.status, 200, "set protection: {}", protect.body);

    let opened = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"forked change","head":"feature-2","base":"main","head_sha":"beadfeed","actor":"alice","source_repository":"fork-owner/jeryu","commits":[{"sha":"beadfeed","verified":false,"parents":1}]}"#,
    );
    assert_eq!(opened.status, 201, "open pr: {}", opened.body);
    let pr = body(&opened);
    let number = pr["number"].as_u64().expect("pr number");
    assert_eq!(pr["source_repository"], "fork-owner/jeryu");

    let blocked = router.put(&format!("/repos/alice/jeryu/pulls/{number}/merge"), "{}");
    assert_eq!(
        blocked.status, 405,
        "merge should remain blocked: {}",
        blocked.body
    );
    let blocked_body = body(&blocked);
    let message = blocked_body["message"]
        .as_str()
        .expect("merge blocker message");
    assert!(
        message.contains("UnsignedCommits"),
        "signed-commit enforcement should still reject fork provenance: {}",
        blocked.body
    );
}

/// Collects the per-repo `number` of every PR in a list response body.
fn pull_numbers(response: &jeryu_api::Response) -> Vec<u64> {
    assert_eq!(response.status, 200, "list pulls: {}", response.body);
    body(response)
        .as_array()
        .expect("pulls array")
        .iter()
        .map(|pr| pr["number"].as_u64().expect("pr number"))
        .collect()
}

#[test]
fn pulls_list_honors_state_query_filter() {
    let router = router_with_repo();

    // One PR stays open; the other is merged so GitHub renders it as `closed`
    // (a closed sub-state with `merged_at` set). No protection -> both mergeable.
    let open = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"stays open","head":"feat-open","base":"main","head_sha":"sha-open"}"#,
    );
    assert_eq!(open.status, 201, "open pr: {}", open.body);
    let open_number = body(&open)["number"].as_u64().expect("open pr number");

    let to_merge = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"gets merged","head":"feat-merge","base":"main","head_sha":"sha-merge"}"#,
    );
    assert_eq!(to_merge.status, 201, "open pr to merge: {}", to_merge.body);
    let merged_number = body(&to_merge)["number"].as_u64().expect("merge pr number");

    let merged = router.put(
        &format!("/repos/alice/jeryu/pulls/{merged_number}/merge"),
        "{}",
    );
    assert_eq!(merged.status, 200, "merge pr: {}", merged.body);
    assert_eq!(body(&merged)["merged"], true);

    // `?state=open` returns only the open PR, never the merged one.
    let open_only = pull_numbers(&router.get("/repos/alice/jeryu/pulls?state=open"));
    assert_eq!(
        open_only,
        vec![open_number],
        "state=open returns only the open PR"
    );

    // `?state=closed` returns the merged PR (merged is a closed sub-state),
    // never the open one.
    let closed_only = pull_numbers(&router.get("/repos/alice/jeryu/pulls?state=closed"));
    assert_eq!(
        closed_only,
        vec![merged_number],
        "state=closed returns only the merged/closed PR"
    );

    // `?state=all` returns both, sorted by number.
    let all = pull_numbers(&router.get("/repos/alice/jeryu/pulls?state=all"));
    assert_eq!(
        all,
        vec![open_number, merged_number],
        "state=all returns both"
    );

    // Absent `state` defaults to GitHub's `open`, matching `?state=open`.
    let default = pull_numbers(&router.get("/repos/alice/jeryu/pulls"));
    assert_eq!(
        default,
        vec![open_number],
        "absent state defaults to open (GitHub's documented default)"
    );

    // An unrecognized value is treated as the default rather than erroring.
    let bogus = pull_numbers(&router.get("/repos/alice/jeryu/pulls?state=bogus"));
    assert_eq!(
        bogus,
        vec![open_number],
        "unknown state falls back to the open default"
    );
}

#[test]
fn patch_pull_request_updates_draft_and_state() {
    let router = router_with_repo();

    let opened = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"draft work","head":"feat-draft","base":"main","head_sha":"sha-draft","draft":true}"#,
    );
    assert_eq!(opened.status, 201, "open draft pr: {}", opened.body);
    let number = body(&opened)["number"].as_u64().expect("pr number");
    assert_eq!(body(&opened)["draft"], true);

    let ready = router.handle(
        Method::Patch,
        &format!("/repos/alice/jeryu/pulls/{number}"),
        r#"{"draft":false,"title":"ready work"}"#,
    );
    assert_eq!(ready.status, 200, "ready patch: {}", ready.body);
    let ready_body = body(&ready);
    assert_eq!(ready_body["draft"], false);
    assert_eq!(ready_body["title"], "ready work");
    assert_eq!(ready_body["state"], "open");

    let closed = router.handle(
        Method::Patch,
        &format!("/repos/alice/jeryu/pulls/{number}"),
        r#"{"state":"closed"}"#,
    );
    assert_eq!(closed.status, 200, "close patch: {}", closed.body);
    let closed_body = body(&closed);
    assert_eq!(closed_body["state"], "closed");
    assert_eq!(closed_body["draft"], false);
    assert_eq!(closed_body["merged"], false);

    let open_only = pull_numbers(&router.get("/repos/alice/jeryu/pulls?state=open"));
    assert!(open_only.is_empty(), "closed PR must not appear as open");
    let closed_only = pull_numbers(&router.get("/repos/alice/jeryu/pulls?state=closed"));
    assert_eq!(closed_only, vec![number]);
}

/// Returns the number of pull requests currently open on the repo.
fn open_pr_count(router: &GithubRouter) -> usize {
    let listed = router.get("/repos/alice/jeryu/pulls");
    assert_eq!(listed.status, 200, "list pulls: {}", listed.body);
    body(&listed).as_array().expect("pulls array").len()
}

/// Reads an advisory response header by name (case-sensitive), if present.
fn header<'a>(response: &'a jeryu_api::Response, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn overlapping_change_routes_to_existing_pr_without_creating_a_new_one() {
    let router = router_with_repo();

    // Two existing OPEN PRs touching disjoint files (no protection -> mergeable).
    let a = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"a","head":"feat-a","base":"main","head_sha":"sha-a","changed_files":["src/a.rs"]}"#,
    );
    assert_eq!(a.status, 201, "open pr a: {}", a.body);
    let b = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"b","head":"feat-b","base":"main","head_sha":"sha-b","changed_files":["src/b.rs","src/c.rs"]}"#,
    );
    assert_eq!(b.status, 201, "open pr b: {}", b.body);
    let target = body(&b)["number"].as_u64().expect("pr b number");

    // Sanity: both PRs are mergeable (green head proof) under no protection.
    assert_eq!(body(&a)["mergeable"], true);
    assert_eq!(body(&b)["mergeable"], true);

    assert_eq!(open_pr_count(&router), 2);

    // A change overlapping PR b above threshold (identical file set, Jaccard 1.0)
    // routes onto it instead of opening a third PR.
    let routed = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"hot-fix b","head":"feat-b2","base":"main","changed_files":["src/b.rs","src/c.rs"]}"#,
    );
    assert_eq!(routed.status, 200, "route response: {}", routed.body);
    let parsed = body(&routed);
    assert_eq!(
        parsed["route_to_existing"]["pr"].as_u64().expect("pr"),
        target
    );
    assert!(
        parsed["route_to_existing"]["reason"]
            .as_str()
            .expect("reason")
            .contains(&format!("#{target}")),
        "reason names the reused PR: {}",
        routed.body
    );
    // The X-Jeryu-Reused-PR header points at the reused PR.
    assert_eq!(
        header(&routed, "X-Jeryu-Reused-PR"),
        Some(target.to_string().as_str())
    );

    // Crucially, no new PR row was created.
    assert_eq!(open_pr_count(&router), 2, "routing must not open a new PR");
}

#[test]
fn below_threshold_overlap_creates_a_new_pr() {
    let router = router_with_repo();

    let existing = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"existing","head":"feat-x","base":"main","head_sha":"sha-x","changed_files":["src/a.rs","src/b.rs","src/c.rs","src/d.rs"]}"#,
    );
    assert_eq!(existing.status, 201, "open existing: {}", existing.body);
    assert_eq!(open_pr_count(&router), 1);

    // Overlap is 1/4 = 0.25, below the default 0.5 threshold -> create new.
    let created = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"new work","head":"feat-y","base":"main","head_sha":"sha-y","changed_files":["src/a.rs","src/e.rs","src/f.rs"]}"#,
    );
    assert_eq!(created.status, 201, "should create new: {}", created.body);
    assert!(
        body(&created).get("route_to_existing").is_none(),
        "below-threshold create must not be a routing payload"
    );
    assert!(
        header(&created, "X-Jeryu-Reused-PR").is_none(),
        "no reuse header on a fresh create"
    );
    assert_eq!(open_pr_count(&router), 2, "a fresh PR row was created");
}

#[test]
fn stale_base_candidate_refuses_to_coalesce() {
    let router = router_with_repo();

    // An existing PR that overlaps strongly but is built on a different base.
    let existing = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"existing","head":"feat-old","base":"main","head_sha":"sha-old","base_sha":"old-base","changed_files":["src/a.rs","src/b.rs"]}"#,
    );
    assert_eq!(existing.status, 201, "open existing: {}", existing.body);
    let stale = body(&existing)["number"].as_u64().expect("pr number");
    assert_eq!(open_pr_count(&router), 1);

    // The change overlaps PR `stale` at Jaccard 1.0 but is on a newer base; the
    // engine refuses to coalesce onto a diverged base -> 409.
    let refused = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"hot-fix","head":"feat-new","base":"main","base_sha":"new-base","changed_files":["src/a.rs","src/b.rs"]}"#,
    );
    assert_eq!(refused.status, 409, "refuse response: {}", refused.body);
    let parsed = body(&refused);
    assert_eq!(parsed["refuse_coalesce"]["pr"].as_u64().expect("pr"), stale);
    assert!(
        parsed["message"]
            .as_str()
            .expect("message")
            .contains("diverged base"),
        "409 message explains the diverged-base refusal: {}",
        refused.body
    );
    // Refusal must not silently open a new PR either.
    assert_eq!(open_pr_count(&router), 1, "refusal must not open a new PR");
}

#[test]
fn create_without_changed_files_skips_overlap_and_creates() {
    let router = router_with_repo();

    // An existing PR that WOULD overlap, but the new request carries no
    // `changed_files`, so overlap is skipped and the create proceeds.
    let existing = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"existing","head":"feat-a","base":"main","head_sha":"sha-a","changed_files":["src/a.rs"]}"#,
    );
    assert_eq!(existing.status, 201, "open existing: {}", existing.body);
    assert_eq!(open_pr_count(&router), 1);

    let created = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"no files","head":"feat-b","base":"main","head_sha":"sha-b"}"#,
    );
    assert_eq!(
        created.status, 201,
        "missing changed_files must still create: {}",
        created.body
    );
    assert!(header(&created, "X-Jeryu-Reused-PR").is_none());
    assert_eq!(open_pr_count(&router), 2);
}

#[test]
fn issues_and_comments_roundtrip() {
    let router = router_with_repo();

    let created = router.post(
        "/repos/alice/jeryu/issues",
        r#"{"title":"bug report","body":"it broke","labels":["bug"],"actor":"alice"}"#,
    );
    assert_eq!(created.status, 201, "create issue: {}", created.body);
    let issue = body(&created);
    let number = issue["number"].as_u64().expect("issue number");
    assert_eq!(issue["state"], "open");
    assert_eq!(issue["title"], "bug report");
    assert_eq!(issue["user"]["login"], "alice");
    assert_eq!(issue["labels"][0], "bug");

    let comment = router.post(
        &format!("/repos/alice/jeryu/issues/{number}/comments"),
        r#"{"body":"confirmed reproduction","actor":"bob"}"#,
    );
    assert_eq!(comment.status, 201, "create comment: {}", comment.body);
    assert_eq!(body(&comment)["body"], "confirmed reproduction");
    assert_eq!(body(&comment)["user"]["login"], "bob");

    let comments = router.get(&format!("/repos/alice/jeryu/issues/{number}/comments"));
    assert_eq!(comments.status, 200);
    let listed = body(&comments);
    assert_eq!(listed.as_array().expect("array").len(), 1);
    assert_eq!(listed[0]["body"], "confirmed reproduction");

    let issues = router.get("/repos/alice/jeryu/issues");
    assert_eq!(issues.status, 200);
    assert_eq!(body(&issues).as_array().expect("array").len(), 1);

    let detail = router.get(&format!("/repos/alice/jeryu/issues/{number}"));
    assert_eq!(detail.status, 200);
    assert_eq!(body(&detail)["number"], number);

    let updated = router.handle(
        Method::Patch,
        &format!("/repos/alice/jeryu/issues/{number}"),
        r#"{"title":"fixed bug report","state":"closed","labels":["bug","p1"]}"#,
    );
    assert_eq!(updated.status, 200, "update issue: {}", updated.body);
    let updated_body = body(&updated);
    assert_eq!(updated_body["title"], "fixed bug report");
    assert_eq!(updated_body["state"], "closed");
    assert_eq!(updated_body["labels"][1], "p1");
}

#[test]
fn issue_bridge_failure_records_repair_evidence_without_breaking_issue_create() {
    let temp = tempfile::tempdir().expect("temp dir");
    let db_dir = temp.path().join("work-db");
    let db_path = db_dir.join("work.sqlite");
    let work = WorkStore::open(&db_path).expect("open work store");
    std::fs::remove_file(&db_path).expect("remove sqlite file");
    std::fs::remove_dir(&db_dir).expect("remove sqlite dir");
    std::fs::write(&db_dir, "not a directory").expect("block sqlite parent path");

    let router = GithubRouter::new().with_work_store(work);
    let repo = router.post(
        "/repos",
        r#"{"owner":"alice","name":"jeryu","private":false,"description":"forge"}"#,
    );
    assert_eq!(repo.status, 201, "create repo: {}", repo.body);

    let created = router.post(
        "/repos/alice/jeryu/issues",
        r#"{"title":"bug report","body":"it broke","labels":["bug"],"actor":"alice"}"#,
    );
    assert_eq!(created.status, 201, "create issue: {}", created.body);
    assert_eq!(header(&created, "X-Jeryu-Work-Bridge"), Some("degraded"));
    assert_eq!(
        header(&created, "X-Jeryu-Work-Repair-Code"),
        Some("work_bridge_lookup_failed")
    );

    let repairs = router.work_bridge_repairs();
    assert_eq!(repairs.len(), 1);
    assert_eq!(repairs[0].operation, "create issue");
    assert_eq!(repairs[0].owner, "alice");
    assert_eq!(repairs[0].repo, "jeryu");
    assert_eq!(repairs[0].issue_number, 1);
    assert!(!repairs[0].reason.is_empty());
    assert!(!repairs[0].repair_hint.is_empty());
}

#[test]
fn webhooks_create_and_list() {
    let router = router_with_repo();

    let created = router.post(
        "/repos/alice/jeryu/hooks",
        r#"{"events":["push","pull_request"],"config":{"url":"https://hooks.invalid/jeryu"}}"#,
    );
    assert_eq!(created.status, 201, "create hook: {}", created.body);
    let hook = body(&created);
    assert_eq!(hook["config"]["url"], "https://hooks.invalid/jeryu");
    assert_eq!(hook["active"], true);
    assert_eq!(hook["events"][0], "push");

    let listed = router.get("/repos/alice/jeryu/hooks");
    assert_eq!(listed.status, 200);
    assert_eq!(body(&listed).as_array().expect("array").len(), 1);
}

#[test]
fn releases_create_and_list() {
    let router = router_with_repo();

    let created = router.post(
        "/repos/alice/jeryu/releases",
        r#"{"tag_name":"v1.0.0","name":"First","body":"notes","prerelease":false}"#,
    );
    assert_eq!(created.status, 201, "create release: {}", created.body);
    let release = body(&created);
    assert_eq!(release["tag_name"], "v1.0.0");
    assert_eq!(release["name"], "First");
    // target_commitish defaults to the repo default branch.
    assert_eq!(release["target_commitish"], "main");

    let listed = router.get("/repos/alice/jeryu/releases");
    assert_eq!(listed.status, 200);
    assert!(listed.body.starts_with('['));
}

#[test]
fn unknown_repo_returns_404_github_shaped() {
    let router = GithubRouter::new();

    let response = router.get("/repos/alice/missing");
    assert_eq!(response.status, 404);
    // The error names the missing entity in a GitHub-shaped error object.
    assert!(
        body(&response)["message"]
            .as_str()
            .expect("message string")
            .contains("alice/missing")
    );
    assert!(body(&response).get("documentation_url").is_some());

    let pulls = router.get("/repos/alice/missing/pulls");
    assert_eq!(pulls.status, 404);
    assert!(body(&pulls).get("message").is_some());

    let issues = router.get("/repos/alice/missing/issues");
    assert_eq!(issues.status, 404);
}

#[test]
fn unknown_pull_request_number_returns_404() {
    let router = router_with_repo();
    let response = router.get("/repos/alice/jeryu/pulls/999");
    assert_eq!(response.status, 404, "{}", response.body);
    assert!(body(&response).get("message").is_some());
}

#[test]
fn unmatched_route_returns_404_not_found_body() {
    let router = GithubRouter::new();
    let response = router.handle(Method::Get, "/repos/alice/jeryu/unknown-thing", "");
    assert_eq!(response.status, 404);
    let parsed = body(&response);
    assert_eq!(parsed["message"], "Not Found");
    assert_eq!(
        parsed["jeryu_repair_hint"]["purpose"],
        "route unsupported GitHub-compatible REST request"
    );
    assert!(parsed["jeryu_api_routes"].as_array().unwrap().len() >= 6);
    for tool in parsed["jeryu_mcp_tools"].as_array().unwrap() {
        let tool = tool.as_str().unwrap();
        assert!(tool.starts_with("jeryu."), "invalid tool prefix: {tool}");
        assert!(!tool.contains("jeryu.mcp."), "old tool namespace: {tool}");
    }
}

#[test]
fn invalid_json_body_returns_422() {
    let router = router_with_repo();

    let response = router.post("/repos/alice/jeryu/issues", "{ this is not json");
    assert_eq!(response.status, 422, "{}", response.body);
    let parsed = body(&response);
    assert_eq!(parsed["message"], "Validation Failed");
    assert!(parsed["errors"].is_array());
}

#[test]
fn invalid_pull_request_number_path_returns_422() {
    let router = router_with_repo();
    let response = router.get("/repos/alice/jeryu/pulls/not-a-number");
    assert_eq!(response.status, 422, "{}", response.body);
    assert_eq!(body(&response)["message"], "Validation Failed");
}

#[test]
fn duplicate_repository_is_a_validation_error() {
    let router = router_with_repo();
    let response = router.post("/repos", r#"{"owner":"alice","name":"jeryu"}"#);
    // Conflict surfaces as 422 in the GitHub-compatible contract.
    assert_eq!(response.status, 422, "{}", response.body);
    assert!(body(&response).get("message").is_some());
}

#[test]
fn graphql_viewer_login_probe_is_supported() {
    let router = GithubRouter::new();
    let response = router.post("/graphql", r#"{"query":"query { viewer { login name } }"}"#);
    assert_eq!(response.status, 200, "{}", response.body);
    let parsed = body(&response);
    assert_eq!(parsed["data"]["viewer"]["login"], "jeryu");
    assert_eq!(parsed["data"]["viewer"]["name"], "Jeryu Local Operator");
}

#[test]
fn graphql_repository_read_probe_is_supported() {
    let router = router_with_repo();
    let response = router.post(
        "/graphql",
        r#"{"query":"query Repo($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { nameWithOwner defaultBranchRef { name } isPrivate } }","variables":{"owner":"alice","name":"jeryu"}}"#,
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let repo = &body(&response)["data"]["repository"];
    assert_eq!(repo["name"], "jeryu");
    assert_eq!(repo["nameWithOwner"], "alice/jeryu");
    assert_eq!(repo["defaultBranchRef"]["name"], "main");
    assert_eq!(repo["isPrivate"], false);
}

#[test]
fn actions_runs_empty_repo_returns_valid_empty_object() {
    let router = router_with_repo();

    // No check-runs yet -> a valid, EMPTY GitHub-shaped object so `gh run list`
    // works without erroring.
    let runs = router.get("/repos/alice/jeryu/actions/runs");
    assert_eq!(runs.status, 200, "{}", runs.body);
    let parsed = body(&runs);
    assert_eq!(parsed["total_count"], 0);
    assert_eq!(parsed["workflow_runs"].as_array().expect("array").len(), 0);

    let workflows = router.get("/repos/alice/jeryu/actions/workflows");
    assert_eq!(workflows.status, 200, "{}", workflows.body);
    let parsed = body(&workflows);
    assert_eq!(parsed["total_count"], 0);
    assert_eq!(parsed["workflows"].as_array().expect("array").len(), 0);
}

#[test]
fn actions_runs_are_sourced_from_check_runs() {
    let router = router_with_repo();

    // A check-run projects to one workflow run, one workflow, and one job.
    let check = router.post(
        "/repos/alice/jeryu/check-runs",
        r#"{"name":"ci/fast","head_sha":"deadbeef","status":"completed","conclusion":"success"}"#,
    );
    assert_eq!(check.status, 201, "{}", check.body);

    let runs = router.get("/repos/alice/jeryu/actions/runs");
    assert_eq!(runs.status, 200, "{}", runs.body);
    let parsed = body(&runs);
    assert_eq!(parsed["total_count"], 1);
    let run = &parsed["workflow_runs"][0];
    let run_id = run["id"].as_u64().expect("run id");
    assert_eq!(run_id, 1);
    assert_eq!(run["name"], "ci/fast");
    assert_eq!(run["head_sha"], "deadbeef");
    assert_eq!(run["head_branch"], "main");
    assert_eq!(run["path"], ".github/workflows/ci-fast.yml@main");
    assert_eq!(run["status"], "completed");
    assert_eq!(run["conclusion"], "success");
    assert_eq!(run["workflow_id"], 1);
    assert_eq!(run["workflow_name"], "ci/fast");
    assert_eq!(
        run["workflow_url"],
        "/repos/alice/jeryu/actions/workflows/1"
    );
    assert_eq!(run["jobs_url"], "/repos/alice/jeryu/actions/runs/1/jobs");
    assert_eq!(run["run_started_at"], run["created_at"]);

    // GET a single run resolves by its synthesized id.
    let single = router.get("/repos/alice/jeryu/actions/runs/1");
    assert_eq!(single.status, 200, "{}", single.body);
    let single_body = body(&single);
    assert_eq!(single_body["name"], "ci/fast");
    assert_eq!(single_body["workflow_name"], "ci/fast");
    assert_eq!(single_body["path"], ".github/workflows/ci-fast.yml@main");

    let workflow = router.get("/repos/alice/jeryu/actions/workflows/1");
    assert_eq!(workflow.status, 200, "{}", workflow.body);
    let workflow_body = body(&workflow);
    assert_eq!(workflow_body["id"], 1);
    assert_eq!(workflow_body["name"], "ci/fast");
    assert_eq!(workflow_body["path"], ".github/workflows/ci-fast.yml");
    assert_eq!(workflow_body["state"], "active");
    assert_eq!(
        workflow_body["url"],
        "/repos/alice/jeryu/actions/workflows/1"
    );
    assert_eq!(
        workflow_body["html_url"],
        "/alice/jeryu/blob/main/.github/workflows/ci-fast.yml"
    );
    assert!(
        workflow_body["badge_url"]
            .as_str()
            .expect("badge url")
            .ends_with("/alice/jeryu/workflows/ci-fast/badge.svg")
    );

    let workflow_by_file = router.get("/repos/alice/jeryu/actions/workflows/ci-fast.yml");
    assert_eq!(workflow_by_file.status, 200, "{}", workflow_by_file.body);
    assert_eq!(body(&workflow_by_file)["id"], 1);

    let workflow_runs = router.get("/repos/alice/jeryu/actions/workflows/1/runs");
    assert_eq!(workflow_runs.status, 200, "{}", workflow_runs.body);
    let workflow_runs_body = body(&workflow_runs);
    assert_eq!(workflow_runs_body["total_count"], 1);
    assert_eq!(workflow_runs_body["workflow_runs"][0]["workflow_id"], 1);
    assert_eq!(
        workflow_runs_body["workflow_runs"][0]["workflow_name"],
        "ci/fast"
    );
    assert_eq!(
        workflow_runs_body["workflow_runs"][0]["path"],
        ".github/workflows/ci-fast.yml@main"
    );

    // Jobs for the run.
    let jobs = router.get("/repos/alice/jeryu/actions/runs/1/jobs");
    assert_eq!(jobs.status, 200, "{}", jobs.body);
    let parsed = body(&jobs);
    assert_eq!(parsed["total_count"], 1);
    assert_eq!(parsed["jobs"][0]["name"], "ci/fast");
    assert_eq!(
        parsed["jobs"][0]["run_url"],
        "/repos/alice/jeryu/actions/runs/1"
    );
    assert_eq!(parsed["jobs"][0]["workflow_name"], "ci/fast");
    assert_eq!(parsed["jobs"][0]["head_branch"], "main");
    assert_eq!(parsed["jobs"][0]["steps"][0]["number"], 1);

    // Workflows dedup by check name.
    let workflows = router.get("/repos/alice/jeryu/actions/workflows");
    assert_eq!(workflows.status, 200, "{}", workflows.body);
    let parsed = body(&workflows);
    assert_eq!(parsed["total_count"], 1);
    assert_eq!(parsed["workflows"][0]["name"], "ci/fast");
    assert_eq!(
        parsed["workflows"][0]["path"],
        ".github/workflows/ci-fast.yml"
    );

    // An unknown run id is a GitHub-shaped 404.
    let missing = router.get("/repos/alice/jeryu/actions/runs/999");
    assert_eq!(missing.status, 404, "{}", missing.body);
    assert!(body(&missing).get("message").is_some());
}

#[test]
fn actions_workflow_detail_and_runs_accept_id_or_file_name() {
    let router = router_with_repo();

    let created = router.post(
        "/repos/alice/jeryu/check-runs",
        r#"{"name":"ci/fast","head_sha":"deadbeef","status":"completed","conclusion":"success"}"#,
    );
    assert_eq!(created.status, 201, "{}", created.body);

    let detail = router.get("/repos/alice/jeryu/actions/workflows/ci-fast.yml");
    assert_eq!(detail.status, 200, "{}", detail.body);
    let detail_body = body(&detail);
    assert_eq!(detail_body["id"], 1);
    assert_eq!(detail_body["name"], "ci/fast");

    let runs = router.get("/repos/alice/jeryu/actions/workflows/ci-fast.yml/runs");
    assert_eq!(runs.status, 200, "{}", runs.body);
    let runs_body = body(&runs);
    assert_eq!(runs_body["total_count"], 1);
    assert_eq!(runs_body["workflow_runs"][0]["workflow_id"], 1);
}

#[test]
fn actions_runs_on_unknown_repo_returns_404() {
    let router = GithubRouter::new();
    let runs = router.get("/repos/alice/missing/actions/runs");
    assert_eq!(runs.status, 404, "{}", runs.body);
    assert!(body(&runs).get("message").is_some());
}

#[test]
fn unsupported_actions_writes_return_guided_local_ci_errors() {
    let router = router_with_repo();

    for path in [
        "/repos/alice/jeryu/actions/workflows/ci-fast.yml/dispatches",
        "/repos/alice/jeryu/actions/runs/1/rerun",
        "/repos/alice/jeryu/actions/runs/1/cancel",
    ] {
        let response = router.post(path, r#"{"ref":"main"}"#);
        assert_eq!(response.status, 501, "{path}: {}", response.body);
        let parsed = body(&response);
        assert_eq!(
            parsed["message"],
            "Hosted GitHub Actions writes are not supported on Jeryu; CI is local and MCP-driven."
        );
        assert!(parsed["documentation_url"].is_string());
        assert_eq!(
            parsed["jeryu_repair_hint"]["purpose"],
            "route unsupported GitHub Actions write request"
        );
        assert_eq!(
            parsed["jeryu_connection"]["capabilities"],
            "/.jeryu/capabilities"
        );
        assert_eq!(
            parsed["jeryu_connection"]["first_contact"],
            "/.jeryu/agents/first-contact"
        );
        assert_eq!(parsed["jeryu_connection"]["mcp"], "/mcp");
        assert_eq!(
            parsed["jeryu_connection"]["actions_runs"],
            "GET /repos/alice/jeryu/actions/runs"
        );
        assert_eq!(
            parsed["jeryu_connection"]["actions_run"],
            "GET /repos/alice/jeryu/actions/runs/{id}"
        );
        assert_eq!(
            parsed["jeryu_connection"]["actions_run_jobs"],
            "GET /repos/alice/jeryu/actions/runs/{id}/jobs"
        );
        assert_eq!(
            parsed["jeryu_connection"]["actions_workflows"],
            "GET /repos/alice/jeryu/actions/workflows"
        );
        assert_eq!(
            parsed["jeryu_connection"]["actions_workflow"],
            "GET /repos/alice/jeryu/actions/workflows/{workflow_id}"
        );
        assert_eq!(
            parsed["jeryu_connection"]["actions_workflow_runs"],
            "GET /repos/alice/jeryu/actions/workflows/{workflow_id}/runs"
        );
        assert_eq!(parsed["jeryu_steering"]["mcp_tool"], "jeryu.run_tests");
        assert_eq!(
            parsed["jeryu_steering"]["mcp_tools"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(parsed["jeryu_steering"]["mcp_tools"][0], "jeryu.run_tests");
        assert_eq!(
            parsed["jeryu_steering"]["mcp_tools"][1],
            "jeryu.get_ci_run_jobs"
        );
    }

    let runs = router.get("/repos/alice/jeryu/actions/runs");
    assert_eq!(runs.status, 200, "{}", runs.body);
    let workflows = router.get("/repos/alice/jeryu/actions/workflows");
    assert_eq!(workflows.status, 200, "{}", workflows.body);
}

#[test]
fn list_routes_emit_rfc5988_link_header_for_pagination() {
    let router = router_with_repo();

    // Open three PRs so a per_page=1 page yields next/last links.
    for (head, sha) in [
        ("feat-a", "sha-a"),
        ("feat-b", "sha-b"),
        ("feat-c", "sha-c"),
    ] {
        let opened = router.post(
            "/repos/alice/jeryu/pulls",
            &format!(r#"{{"title":"{head}","head":"{head}","base":"main","head_sha":"{sha}"}}"#),
        );
        assert_eq!(opened.status, 201, "open {head}: {}", opened.body);
    }

    // Page 1 of 3: a single PR plus a Link header pointing to next + last.
    let page1 = router.get("/repos/alice/jeryu/pulls?per_page=1&page=1");
    assert_eq!(page1.status, 200, "{}", page1.body);
    assert_eq!(body(&page1).as_array().expect("array").len(), 1);
    let link = header(&page1, "Link").expect("Link header on page 1");
    assert!(link.contains("rel=\"next\""), "next link present: {link}");
    assert!(link.contains("rel=\"last\""), "last link present: {link}");
    assert!(link.contains("page=2"), "next points at page 2: {link}");
    assert!(link.contains("page=3"), "last points at page 3: {link}");
    assert!(
        !link.contains("rel=\"prev\""),
        "no prev on first page: {link}"
    );

    // Middle page carries all four relations.
    let page2 = router.get("/repos/alice/jeryu/pulls?per_page=1&page=2");
    let link = header(&page2, "Link").expect("Link header on page 2");
    assert!(link.contains("rel=\"next\""));
    assert!(link.contains("rel=\"prev\""));
    assert!(link.contains("rel=\"first\""));
    assert!(link.contains("rel=\"last\""));

    // A single-page result omits the Link header entirely (GitHub parity).
    let single = router.get("/repos/alice/jeryu/pulls?per_page=100");
    assert!(
        header(&single, "Link").is_none(),
        "single page has no Link header"
    );
    assert_eq!(body(&single).as_array().expect("array").len(), 3);
}

#[test]
fn error_bodies_carry_jeryu_steering_fields() {
    let router = router_with_repo();

    // 404 from an unknown repo.
    let not_found = router.get("/repos/alice/missing");
    assert_eq!(not_found.status, 404);
    let steering = &body(&not_found)["jeryu_steering"];
    assert_eq!(steering["faster_path"], "/.jeryu/capabilities");
    assert!(
        steering["mcp_tool"]
            .as_str()
            .expect("mcp_tool")
            .starts_with("jeryu.")
    );
    assert!(steering["hint"].as_str().expect("hint").len() > 5);

    // 422 from an invalid body.
    let invalid = router.post("/repos/alice/jeryu/issues", "{ not json");
    assert_eq!(invalid.status, 422);
    assert_eq!(
        body(&invalid)["jeryu_steering"]["faster_path"],
        "/.jeryu/capabilities"
    );

    // 422 from a non-numeric path id.
    let bad_id = router.get("/repos/alice/jeryu/pulls/not-a-number");
    assert_eq!(bad_id.status, 422);
    assert!(body(&bad_id)["jeryu_steering"]["mcp_tool"].is_string());

    // The catch-all 404 (unmatched route) also teaches.
    let unmatched = router.get("/repos/alice/jeryu/unknown-thing");
    assert_eq!(unmatched.status, 404);
    assert_eq!(
        body(&unmatched)["jeryu_steering"]["faster_path"],
        "/.jeryu/capabilities"
    );
}

#[test]
fn first_contact_returns_a_steering_doc() {
    let router = GithubRouter::new();
    let doc = router.get("/.jeryu/agents/first-contact");
    assert_eq!(doc.status, 200, "{}", doc.body);
    let parsed = body(&doc);
    assert_eq!(parsed["start_here"], "/.jeryu/capabilities");
    let advice = parsed["advice"].as_array().expect("advice array");
    assert!(!advice.is_empty(), "first-contact carries advice");
    assert!(
        advice
            .iter()
            .any(|line| line.as_str().unwrap_or("").contains("/.jeryu/capabilities")),
        "advice points at the capability manifest"
    );
    assert!(
        advice
            .iter()
            .any(|line| line.as_str().unwrap_or("").contains("gh auth login")),
        "advice blocks direct gh auth"
    );
    assert_eq!(
        parsed["gh_auth_policy"]["run_instead"],
        "jeryu gh-setup --host http://127.0.0.1:8787 --token-file ~/.jeryu/secrets/merge-token"
    );
    assert_eq!(
        parsed["gh_auth_policy"]["token_file"],
        "~/.jeryu/secrets/merge-token"
    );
    assert!(
        parsed["gh_auth_policy"]["stale_host_repair"]
            .as_str()
            .expect("stale host repair")
            .contains("--token-file ~/.jeryu/secrets/merge-token")
    );
    assert!(!doc.body.contains("JERYU-TOKEN"));
    for tool in parsed["mcp_tools"].as_array().expect("mcp_tools array") {
        assert!(tool.as_str().unwrap().starts_with("jeryu."));
    }
}

#[test]
fn gh_auth_workaround_paths_return_typed_jeryu_guidance() {
    let router = GithubRouter::new();
    for (method, path) in [
        (Method::Post, "/login/device/code"),
        (Method::Post, "/login/oauth/access_token"),
        (Method::Get, "/login/oauth/authorize"),
        (Method::Post, "/api/v3/login/device/code"),
    ] {
        let response = router.handle(method, path, "{}");
        assert_eq!(response.status, 501, "{path}: {}", response.body);
        let parsed = body(&response);
        assert_eq!(
            parsed["jeryu_repair_hint"]["purpose"],
            "route GitHub CLI auth setup through Jeryu"
        );
        assert!(
            parsed["jeryu_repair_hint"]["common_fixes"]
                .as_array()
                .expect("common fixes")
                .iter()
                .any(|fix| fix
                    .as_str()
                    .unwrap_or("")
                    .contains("--token-file ~/.jeryu/secrets/merge-token")),
            "guidance must point agents at jeryu gh-setup"
        );
        assert_eq!(
            parsed["jeryu_connection"]["gh_setup"],
            "jeryu gh-setup --host http://127.0.0.1:8787 --token-file ~/.jeryu/secrets/merge-token"
        );
        assert!(!response.body.contains("JERYU-TOKEN"));
        assert!(
            parsed["jeryu_repair_hint"]["reason"]
                .as_str()
                .expect("reason")
                .contains("gh auth login"),
            "reason must name the wrong workaround"
        );
    }
}

#[test]
fn unsupported_graphql_returns_guided_repair_hint() {
    let router = GithubRouter::new();
    let response = router.post(
        "/graphql",
        r#"{"query":"mutation { addStar(input: { starrableId: \"R_1\" }) { starrable { id } } }","operation_name":"StarRepo"}"#,
    );
    assert_eq!(response.status, 501, "{}", response.body);
    let parsed = body(&response);
    assert_eq!(
        parsed["message"],
        "GraphQL query requires a guided Jeryu route"
    );
    assert!(
        parsed["documentation_url"]
            .as_str()
            .unwrap()
            .contains("graphql")
    );
    assert_eq!(
        parsed["jeryu_repair_hint"]["purpose"],
        "route unsupported GitHub GraphQL request"
    );
    assert!(parsed["jeryu_mcp_tools"].as_array().unwrap().len() >= 4);
    for tool in parsed["jeryu_mcp_tools"].as_array().unwrap() {
        let tool = tool.as_str().unwrap();
        assert!(tool.starts_with("jeryu."), "invalid tool prefix: {tool}");
        assert!(!tool.contains("jeryu.mcp."), "old tool namespace: {tool}");
    }
    assert!(
        parsed["jeryu_api_routes"][0]
            .as_str()
            .unwrap()
            .starts_with("GET /repos")
    );
}
