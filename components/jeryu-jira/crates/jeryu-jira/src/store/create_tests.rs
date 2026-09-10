use super::*;

fn issue(number: u64) -> WorkIssueLink {
    WorkIssueLink {
        owner: "alice".to_string(),
        repo: "jeryu".to_string(),
        number,
        url: Some(format!("/repos/jeryu/alice/jeryu/issues#{number}")),
    }
}

#[test]
fn conflicting_linked_creation_preserves_original_and_creates_no_unlinked_item() {
    for recreate_repository in [false, true] {
        let (database, store) = store();
        let original = store
            .create_with_issue(
                CreateWorkItemRequest {
                    repo: Some(repo()),
                    title: "Original issue mirror".to_string(),
                    ..CreateWorkItemRequest::default()
                },
                issue(1),
            )
            .expect("initial linked item");
        store
            .add_comment(
                &original.key,
                CreateWorkCommentRequest {
                    body: "Preserve this history".to_string(),
                    author: None,
                },
            )
            .expect("original comment");
        let before = store.detail(&original.key).expect("original detail");
        let mut target = repo();
        if recreate_repository {
            target.id = "replacement-repository-id".to_string();
        }
        let request = CreateWorkItemRequest {
            repo: Some(target.clone()),
            title: "Conflicting mirror must not persist".to_string(),
            ..CreateWorkItemRequest::default()
        };
        assert!(matches!(
            store.create_with_issue(request, issue(1)),
            Err(WorkError::Conflict(_))
        ));

        let reopened = WorkStore::open(database.path()).expect("reopen after failure");
        assert_eq!(
            reopened
                .list(WorkFilter::default())
                .expect("unchanged list"),
            vec![original.clone()]
        );
        assert_eq!(
            reopened.detail(&original.key).expect("preserved detail"),
            before
        );
        assert_eq!(
            reopened
                .find_by_issue("alice", "jeryu", 1)
                .expect("preserved link"),
            Some(original.clone())
        );
        assert_eq!(original.repo, Some(repo()));

        let successor = reopened
            .create_with_issue(
                CreateWorkItemRequest {
                    repo: Some(target.clone()),
                    title: "Next valid issue mirror".to_string(),
                    ..CreateWorkItemRequest::default()
                },
                issue(2),
            )
            .expect("valid creation after conflict");
        assert_eq!(successor.number, original.number + 1);
        assert_eq!(successor.repo, Some(target));
        assert_eq!(successor.issue, Some(issue(2)));
    }
}

#[test]
fn unrepresentable_link_number_does_not_commit_an_unlinked_creation() {
    let (database, store) = store();
    let request = CreateWorkItemRequest {
        repo: Some(repo()),
        title: "Link cannot fit SQLite integer".to_string(),
        ..CreateWorkItemRequest::default()
    };
    assert!(matches!(
        store.create_with_issue(request, issue(u64::MAX)),
        Err(WorkError::Storage(_))
    ));
    let reopened = WorkStore::open(database.path()).expect("reopen after failed SQL binding");
    assert!(
        reopened
            .list(WorkFilter::default())
            .expect("empty store")
            .is_empty()
    );
    let valid = reopened
        .create_with_issue(
            CreateWorkItemRequest {
                title: "Valid linked item".to_string(),
                ..CreateWorkItemRequest::default()
            },
            issue(1),
        )
        .expect("valid insertion after failure");
    assert_eq!(valid.key, "JRY-1");
    assert_eq!(valid.issue, Some(issue(1)));
}
