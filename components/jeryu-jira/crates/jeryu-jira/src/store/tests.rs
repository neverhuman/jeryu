#[path = "../../tests/support/mod.rs"]
mod support;

use support::TestDatabase;

use super::WorkStore;
use crate::{
    CreateWorkCommentRequest, CreateWorkItemRequest, CreateWorkLinkRequest, UpdateWorkItemRequest,
    WorkError, WorkFilter, WorkIssueLink, WorkItemKind, WorkPrincipal, WorkPrincipalKind,
    WorkPriority, WorkPullRequestLink, WorkRepository, WorkStatus,
};

std::thread_local! {
    static AFTER_DETAIL_ITEM: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn after_detail_item_read() {
    let callback = AFTER_DETAIL_ITEM.with(|hook| hook.borrow_mut().take());
    if let Some(callback) = callback {
        callback();
    }
}

fn store() -> (TestDatabase, WorkStore) {
    let database = TestDatabase::temporary();
    let store = WorkStore::open(database.path()).expect("open store");
    (database, store)
}

fn repo() -> WorkRepository {
    WorkRepository {
        id: "repo-1".to_string(),
        host: "jeryu".to_string(),
        owner: "alice".to_string(),
        name: "jeryu".to_string(),
    }
}

#[test]
fn create_patch_comment_and_reopen_persist() {
    let database = TestDatabase::temporary();
    let path = database.path().to_path_buf();
    let store = WorkStore::open(&path).expect("open store");
    let item = store
        .create(CreateWorkItemRequest {
            repo: Some(repo()),
            title: "Fix CI".to_string(),
            kind: Some(WorkItemKind::Ci),
            priority: Some(WorkPriority::P1),
            labels: vec!["ci".to_string(), "ci".to_string()],
            ..CreateWorkItemRequest::default()
        })
        .expect("create item");
    assert_eq!(item.key, "JRY-1");
    assert_eq!(item.labels, vec!["ci"]);

    let updated = store
        .patch(
            &item.key,
            UpdateWorkItemRequest {
                status: Some(WorkStatus::InProgress),
                assignees: Some(vec![WorkPrincipal {
                    kind: WorkPrincipalKind::Agent,
                    id: "agent-build".to_string(),
                    display_name: None,
                }]),
                ..UpdateWorkItemRequest::default()
            },
        )
        .expect("patch item");
    assert_eq!(updated.status, WorkStatus::InProgress);
    assert_eq!(updated.assignees[0].id, "agent-build");

    store
        .add_comment(
            &item.key,
            CreateWorkCommentRequest {
                body: "Started".to_string(),
                author: None,
            },
        )
        .expect("comment");

    let reopened = WorkStore::open(&path).expect("reopen store");
    let detail = reopened.detail(&item.key).expect("detail");
    assert_eq!(detail.item.status, WorkStatus::InProgress);
    assert_eq!(detail.comments.len(), 1);
    assert_eq!(detail.comments[0].author.id, "local");
}

#[test]
fn detail_keeps_item_and_comments_in_one_snapshot_during_link_changes() {
    let (_database, store) = store();
    let connection = store.connect().expect("writer setup connection");
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .expect("allow writer commits while a reader holds its snapshot");
    let item = store
        .create(CreateWorkItemRequest {
            repo: Some(repo()),
            title: "Snapshot fixture".to_string(),
            ..CreateWorkItemRequest::default()
        })
        .expect("create item");
    store
        .add_comment(
            &item.key,
            CreateWorkCommentRequest {
                body: "Comment before the link".to_string(),
                author: None,
            },
        )
        .expect("initial comment");
    let expected_link = WorkIssueLink {
        owner: "another-owner".to_string(),
        repo: "private-repository".to_string(),
        number: 7,
        url: None,
    };
    let writer = store.clone();
    let key = item.key.clone();
    let link = expected_link.clone();
    AFTER_DETAIL_ITEM.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            writer
                .link(
                    &key,
                    CreateWorkLinkRequest {
                        issue: Some(link),
                        pull_request: None,
                    },
                )
                .expect("writer commits changed access binding");
            writer
                .add_comment(
                    &key,
                    CreateWorkCommentRequest {
                        body: "Comment after the link".to_string(),
                        author: None,
                    },
                )
                .expect("writer commits later comment");
        }));
    });

    let before = store.detail(&item.key).expect("coherent initial snapshot");
    assert!(before.item.issue.is_none());
    assert_eq!(before.comments.len(), 1);
    assert_eq!(before.comments[0].body, "Comment before the link");

    let after = store
        .detail(&item.key)
        .expect("coherent subsequent snapshot");
    assert_eq!(after.item.issue, Some(expected_link));
    assert_eq!(after.comments.len(), 2);
    assert!(
        after
            .comments
            .iter()
            .any(|comment| comment.body == "Comment after the link")
    );
}

#[test]
fn issue_links_are_unique() {
    let (_database, store) = store();
    let first = store
        .create(CreateWorkItemRequest {
            title: "First".to_string(),
            ..CreateWorkItemRequest::default()
        })
        .expect("first");
    let second = store
        .create(CreateWorkItemRequest {
            title: "Second".to_string(),
            ..CreateWorkItemRequest::default()
        })
        .expect("second");
    let issue = WorkIssueLink {
        owner: "alice".to_string(),
        repo: "jeryu".to_string(),
        number: 7,
        url: None,
    };
    store
        .link(
            &first.key,
            CreateWorkLinkRequest {
                issue: Some(issue.clone()),
                pull_request: None,
            },
        )
        .expect("first link");
    let err = store
        .link(
            &second.key,
            CreateWorkLinkRequest {
                issue: Some(issue),
                pull_request: None,
            },
        )
        .expect_err("duplicate issue link fails");
    assert!(matches!(err, WorkError::Conflict(_)));
}

#[test]
fn pull_request_links_are_deduplicated() {
    let (_database, store) = store();
    let item = store
        .create(CreateWorkItemRequest {
            title: "Track PR".to_string(),
            ..CreateWorkItemRequest::default()
        })
        .expect("create");
    let pr = WorkPullRequestLink {
        owner: "alice".to_string(),
        repo: "jeryu".to_string(),
        number: 12,
        url: Some("/alice/jeryu/pulls/12".to_string()),
    };
    store
        .link(
            &item.key,
            CreateWorkLinkRequest {
                issue: None,
                pull_request: Some(pr.clone()),
            },
        )
        .expect("first link");
    let linked = store
        .link(
            &item.key,
            CreateWorkLinkRequest {
                issue: None,
                pull_request: Some(pr),
            },
        )
        .expect("second link");
    assert_eq!(linked.pull_requests.len(), 1);
}

#[test]
fn filter_matches_repo_status_assignee_label_and_search() {
    let (_database, store) = store();
    let created = store
        .create(CreateWorkItemRequest {
            repo: Some(repo()),
            title: "Repair release notes".to_string(),
            body: Some("Markdown drift".to_string()),
            status: Some(WorkStatus::Ready),
            labels: vec!["docs".to_string()],
            assignees: vec![WorkPrincipal {
                kind: WorkPrincipalKind::Human,
                id: "alex".to_string(),
                display_name: None,
            }],
            ..CreateWorkItemRequest::default()
        })
        .expect("create");
    let listed = store
        .list(WorkFilter {
            repo_id: Some("repo-1".to_string()),
            status: Some(WorkStatus::Ready),
            assignee: Some("alex".to_string()),
            label: Some("docs".to_string()),
            search: Some("release".to_string()),
            ..WorkFilter::default()
        })
        .expect("list");
    assert_eq!(listed, vec![created]);
}

#[test]
fn validates_titles_comments_and_links() {
    let (_database, store) = store();
    assert!(matches!(
        store.create(CreateWorkItemRequest {
            title: " ".to_string(),
            ..CreateWorkItemRequest::default()
        }),
        Err(WorkError::Validation(_))
    ));
    let item = store
        .create(CreateWorkItemRequest {
            title: "Valid".to_string(),
            ..CreateWorkItemRequest::default()
        })
        .expect("valid");
    assert!(matches!(
        store.add_comment(
            &item.key,
            CreateWorkCommentRequest {
                body: "".to_string(),
                author: None,
            },
        ),
        Err(WorkError::Validation(_))
    ));
    assert!(matches!(
        store.link(&item.key, CreateWorkLinkRequest::default()),
        Err(WorkError::Validation(_))
    ));
}

#[path = "create_tests.rs"]
mod creation;
