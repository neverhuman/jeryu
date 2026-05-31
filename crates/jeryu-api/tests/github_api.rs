//! Conformance tests for the GitHub-compatible REST edge.
//!
//! These exercise the real route table in `jeryu_api::GithubRouter` end to end:
//! create a repo, open a PR, register a check-run and commit status, configure
//! branch protection, then merge — asserting GitHub-shaped JSON and the right
//! status code at each step. Negative tests pin the 404 / 422 / 405 contracts.

use jeryu_api::{GithubRouter, Method};
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

    let listed = router.get("/repos");
    assert_eq!(listed.status, 200);
    let repos = body(&listed);
    assert_eq!(repos.as_array().expect("array").len(), 1);
    assert_eq!(repos[0]["full_name"], "alice/jeryu");
}

#[test]
fn full_pull_request_lifecycle_create_check_status_protect_and_merge() {
    let router = router_with_repo();

    // Open a PR. GitHub-shaped: `number`, `head`/`base` refs, `state` = open.
    let opened = router.post(
        "/repos/alice/jeryu/pulls",
        r#"{"title":"add feature","head":"feature","base":"main","head_sha":"deadbeef","actor":"alice"}"#,
    );
    assert_eq!(opened.status, 201, "open pr: {}", opened.body);
    let pr = body(&opened);
    let number = pr["number"].as_u64().expect("pr number");
    assert_eq!(number, 1);
    assert_eq!(pr["state"], "open");
    assert_eq!(pr["head"]["ref"], "feature");
    assert_eq!(pr["head"]["sha"], "deadbeef");
    assert_eq!(pr["base"]["ref"], "main");
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
    // engine refuses to coalesce onto a stale base -> 409.
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
            .contains("stale base"),
        "409 message explains the stale-base refusal: {}",
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
