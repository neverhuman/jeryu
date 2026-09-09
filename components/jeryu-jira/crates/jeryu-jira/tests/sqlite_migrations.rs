use jeryu_jira::{CreateWorkItemRequest, WorkStore};
use rusqlite::{Connection, params};

#[test]
fn opening_a_fresh_sqlite_file_applies_the_schema() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("work.sqlite");
    let store = WorkStore::open(&path).expect("open store");
    let item = store
        .create(CreateWorkItemRequest {
            title: "Track split work".to_string(),
            ..CreateWorkItemRequest::default()
        })
        .expect("create work item");
    assert_eq!(item.key, "JRY-1");

    let reopened = WorkStore::open(&path).expect("reopen store");
    assert_eq!(reopened.get("JRY-1").expect("load item").title, item.title);
}

#[test]
fn opening_an_existing_database_is_idempotent() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("work.sqlite");
    WorkStore::open(&path).expect("first open");
    WorkStore::open(&path).expect("second open");
}

#[test]
fn runtime_schema_contains_expected_indexes() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("work.sqlite");
    WorkStore::open(&path).expect("open store");
    let conn = Connection::open(path).expect("open sqlite");
    for index in [
        "work_items_issue_unique",
        "work_items_repo_idx",
        "work_items_status_idx",
        "work_comments_work_idx",
    ] {
        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                params![index],
                |row| row.get(0),
            )
            .expect("index lookup");
        assert_eq!(count, 1, "missing index {index}");
    }
}

#[test]
fn runtime_schema_rejects_invalid_status_and_issue_numbers() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("work.sqlite");
    WorkStore::open(&path).expect("open store");
    let conn = Connection::open(path).expect("open sqlite");
    let err = conn
        .execute(
            r#"
            INSERT INTO work_items (
                id, title, status, kind, priority, labels_json, assignees_json,
                issue_owner, issue_repo, issue_number, pull_requests_json, created_at, updated_at
            ) VALUES (
                'bad-status', 'Bad', 'triaged', 'task', 'p2', '[]', '[]',
                NULL, NULL, NULL, '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
            )
            "#,
            [],
        )
        .expect_err("invalid status fails");
    assert!(err.to_string().contains("CHECK"));

    let err = conn
        .execute(
            r#"
            INSERT INTO work_items (
                id, title, status, kind, priority, labels_json, assignees_json,
                issue_owner, issue_repo, issue_number, pull_requests_json, created_at, updated_at
            ) VALUES (
                'bad-issue', 'Bad', 'backlog', 'task', 'p2', '[]', '[]',
                'alice', 'jeryu', 0, '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
            )
            "#,
            [],
        )
        .expect_err("invalid issue number fails");
    assert!(err.to_string().contains("CHECK"));
}
