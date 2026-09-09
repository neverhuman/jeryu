mod support;

use std::collections::BTreeSet;

use jeryu_jira::{
    CreateWorkItemRequest, CreateWorkLinkRequest, WorkError, WorkIssueLink, WorkStore,
};
use proptest::prelude::*;
use support::TestDatabase;

fn store() -> (TestDatabase, WorkStore) {
    let database = TestDatabase::temporary();
    let store = WorkStore::open(database.path()).expect("open store");
    (database, store)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn create_normalizes_labels(labels in prop::collection::vec("[a-zA-Z0-9 _-]{0,12}", 0..20)) {
        let (_database, store) = store();
        let item = store
            .create(CreateWorkItemRequest {
                title: "Normalize labels".to_string(),
                labels: labels.clone(),
                ..CreateWorkItemRequest::default()
            })
            .expect("create");
        let expected: Vec<String> = labels
            .into_iter()
            .map(|label| label.trim().to_string())
            .filter(|label| !label.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        prop_assert_eq!(item.labels, expected);
    }

    #[test]
    fn issue_links_require_positive_numbers(owner in "[a-z][a-z0-9_-]{0,12}", repo in "[a-z][a-z0-9_-]{0,12}") {
        let (_database, store) = store();
        let item = store
            .create(CreateWorkItemRequest {
                title: "Validate issue".to_string(),
                ..CreateWorkItemRequest::default()
            })
            .expect("create");
        let err = store
            .link(
                &item.key,
                CreateWorkLinkRequest {
                    issue: Some(WorkIssueLink {
                        owner,
                        repo,
                        number: 0,
                        url: None,
                    }),
                    pull_request: None,
                },
            )
            .expect_err("zero issue number fails");
        prop_assert!(matches!(err, WorkError::Validation(_)));
    }
}
