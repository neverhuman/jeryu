//! Behavioral tests for the core forge domain CRUD surface: users, orgs,
//! teams, repositories, labels, issues, milestones, and comments.
//!
//! These assert GitHub-shaped semantics (e.g. issues carry a `number`, repos a
//! `full_name`, the default branch is `main`) using only the public API of
//! `jeryu_core::ForgeCore`.

use jeryu_core::{
    CreateCommentRequest, CreateIssueRequest, CreateLabelRequest, CreateOrganizationRequest,
    CreateRepositoryRequest, CreateTeamRequest, CreateUserRequest, ForgeCore, ForgeError,
    IssueState, UpdateIssueRequest,
};

fn core() -> ForgeCore {
    ForgeCore::new()
}

fn core_with_repo(owner: &str, repo: &str) -> ForgeCore {
    let core = core();
    core.create_user(CreateUserRequest {
        login: owner.to_string(),
        ..Default::default()
    })
    .unwrap();
    core.create_repository(
        owner,
        CreateRepositoryRequest {
            name: repo.to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    core
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

#[test]
fn create_and_get_user_roundtrips_fields() {
    let core = core();
    let created = core
        .create_user(CreateUserRequest {
            login: "octocat".to_string(),
            name: Some("The Octocat".to_string()),
            email: Some("octo@example.test".to_string()),
        })
        .unwrap();

    assert_eq!(created.login, "octocat");
    assert_eq!(created.name.as_deref(), Some("The Octocat"));
    assert_eq!(created.email.as_deref(), Some("octo@example.test"));

    let fetched = core.get_user("octocat").unwrap();
    assert_eq!(fetched, created);
    // The id is a freshly generated UUID, not a sequential integer.
    assert!(!fetched.id.is_nil());
}

#[test]
fn duplicate_user_login_is_a_conflict() {
    let core = core();
    core.create_user(CreateUserRequest {
        login: "dup".to_string(),
        ..Default::default()
    })
    .unwrap();
    let err = core
        .create_user(CreateUserRequest {
            login: "dup".to_string(),
            ..Default::default()
        })
        .unwrap_err();
    assert!(matches!(err, ForgeError::Conflict(_)));
}

#[test]
fn empty_user_login_is_rejected() {
    let core = core();
    let err = core
        .create_user(CreateUserRequest {
            login: "   ".to_string(),
            ..Default::default()
        })
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn get_unknown_user_is_not_found() {
    let core = core();
    let err = core.get_user("ghost").unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn ensure_user_is_idempotent() {
    let core = core();
    let first = core.ensure_user("lazy");
    let second = core.ensure_user("lazy");
    // ensure_user must not mint a new identity on the second call.
    assert_eq!(first.id, second.id);
    assert_eq!(core.get_user("lazy").unwrap().id, first.id);
}

// ---------------------------------------------------------------------------
// Organizations + Teams
// ---------------------------------------------------------------------------

#[test]
fn create_and_get_organization() {
    let core = core();
    let org = core
        .create_organization(CreateOrganizationRequest {
            login: "acme".to_string(),
            display_name: Some("Acme Inc".to_string()),
        })
        .unwrap();
    assert_eq!(org.login, "acme");
    assert_eq!(org.display_name.as_deref(), Some("Acme Inc"));
    assert_eq!(core.get_organization("acme").unwrap(), org);
}

#[test]
fn duplicate_organization_is_conflict() {
    let core = core();
    core.create_organization(CreateOrganizationRequest {
        login: "acme".to_string(),
        display_name: None,
    })
    .unwrap();
    let err = core
        .create_organization(CreateOrganizationRequest {
            login: "acme".to_string(),
            display_name: None,
        })
        .unwrap_err();
    assert!(matches!(err, ForgeError::Conflict(_)));
}

#[test]
fn team_requires_existing_organization() {
    let core = core();
    let err = core
        .create_team(
            "no-such-org",
            CreateTeamRequest {
                name: "Core".to_string(),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn team_slug_is_derived_when_absent_and_lists_per_org() {
    let core = core();
    core.create_organization(CreateOrganizationRequest {
        login: "acme".to_string(),
        display_name: None,
    })
    .unwrap();

    let team = core
        .create_team(
            "acme",
            CreateTeamRequest {
                name: "Platform Eng".to_string(),
                slug: None,
                members: vec!["alice".to_string(), "bob".to_string()],
            },
        )
        .unwrap();
    // "Platform Eng" slugifies to "platform-eng".
    assert_eq!(team.slug, "platform-eng");
    assert_eq!(team.organization, "acme");
    assert_eq!(team.members, vec!["alice".to_string(), "bob".to_string()]);

    let teams = core.list_teams("acme").unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].slug, "platform-eng");
}

#[test]
fn duplicate_team_slug_in_same_org_conflicts() {
    let core = core();
    core.create_organization(CreateOrganizationRequest {
        login: "acme".to_string(),
        display_name: None,
    })
    .unwrap();
    core.create_team(
        "acme",
        CreateTeamRequest {
            name: "Core".to_string(),
            slug: Some("core".to_string()),
            members: vec![],
        },
    )
    .unwrap();
    let err = core
        .create_team(
            "acme",
            CreateTeamRequest {
                name: "Core Two".to_string(),
                slug: Some("core".to_string()),
                members: vec![],
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Conflict(_)));
}

#[test]
fn list_teams_unknown_org_is_not_found() {
    let core = core();
    assert!(matches!(
        core.list_teams("nope").unwrap_err(),
        ForgeError::NotFound(_)
    ));
}

// ---------------------------------------------------------------------------
// Repositories
// ---------------------------------------------------------------------------

#[test]
fn repository_defaults_to_main_branch_and_full_name() {
    let core = core();
    core.create_user(CreateUserRequest {
        login: "alice".to_string(),
        ..Default::default()
    })
    .unwrap();
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: Some("a forge".to_string()),
                default_branch: None,
            },
        )
        .unwrap();

    assert_eq!(repo.owner, "alice");
    assert_eq!(repo.name, "jeryu");
    assert_eq!(repo.full_name, "alice/jeryu");
    assert_eq!(repo.default_branch, "main");
    assert!(repo.private);
    assert!(!repo.archived);
    assert!(!repo.disabled);
}

#[test]
fn repository_custom_default_branch_is_respected() {
    let core = core_with_repo("alice", "trunk-repo");
    // Recreate with custom branch under a different name.
    let repo = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "legacy".to_string(),
                private: false,
                description: None,
                default_branch: Some("trunk".to_string()),
            },
        )
        .unwrap();
    assert_eq!(repo.default_branch, "trunk");
}

#[test]
fn repository_creation_seeds_linear_history_protection_on_the_default_branch() {
    let core = core();
    core.create_user(CreateUserRequest {
        login: "alice".to_string(),
        ..Default::default()
    })
    .unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "linear".to_string(),
            default_branch: Some("trunk".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let rule = core
        .get_branch_protection("alice", "linear", "trunk")
        .unwrap();
    assert!(rule.required_linear_history);
    assert!(rule.required_status_checks.is_empty());
    assert_eq!(rule.required_approving_review_count, 0);
    assert!(!rule.allow_force_pushes);
    assert!(!rule.allow_deletions);
    assert_eq!(rule.branch, "trunk");
}

#[test]
fn duplicate_repository_conflicts_but_other_owner_is_fine() {
    let core = core_with_repo("alice", "jeryu");
    let err = core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Conflict(_)));

    // Same repo name under a different owner is a distinct repository.
    core.create_repository(
        "bob",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(core.get_repository("bob", "jeryu").unwrap().owner, "bob");
}

#[test]
fn list_repositories_filters_by_owner_and_sorts() {
    let core = core();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "zeta".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "alpha".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    core.create_repository(
        "bob",
        CreateRepositoryRequest {
            name: "gamma".to_string(),
            ..Default::default()
        },
    )
    .unwrap();

    let alice = core.list_repositories(Some("alice"));
    assert_eq!(alice.len(), 2);
    // sorted by full_name: alice/alpha before alice/zeta
    assert_eq!(alice[0].full_name, "alice/alpha");
    assert_eq!(alice[1].full_name, "alice/zeta");

    let all = core.list_repositories(None);
    assert_eq!(all.len(), 3);
}

#[test]
fn get_unknown_repository_is_not_found() {
    let core = core();
    assert!(matches!(
        core.get_repository("nobody", "nothing").unwrap_err(),
        ForgeError::NotFound(_)
    ));
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

#[test]
fn label_uses_default_color_and_lists_sorted() {
    let core = core_with_repo("alice", "jeryu");
    let bug = core
        .create_label(
            "alice",
            "jeryu",
            CreateLabelRequest {
                name: "bug".to_string(),
                color: "d73a4a".to_string(),
                description: Some("Something is broken".to_string()),
            },
        )
        .unwrap();
    assert_eq!(bug.color, "d73a4a");

    core.create_label(
        "alice",
        "jeryu",
        CreateLabelRequest {
            name: "aaa".to_string(),
            color: jeryu_core::default_label_color(),
            description: None,
        },
    )
    .unwrap();

    let labels = core.list_labels("alice", "jeryu").unwrap();
    assert_eq!(labels.len(), 2);
    // sorted alphabetically by name
    assert_eq!(labels[0].name, "aaa");
    assert_eq!(labels[1].name, "bug");
}

#[test]
fn duplicate_label_conflicts() {
    let core = core_with_repo("alice", "jeryu");
    core.create_label(
        "alice",
        "jeryu",
        CreateLabelRequest {
            name: "bug".to_string(),
            color: "ededed".to_string(),
            description: None,
        },
    )
    .unwrap();
    let err = core
        .create_label(
            "alice",
            "jeryu",
            CreateLabelRequest {
                name: "bug".to_string(),
                color: "000000".to_string(),
                description: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Conflict(_)));
}

#[test]
fn label_on_missing_repository_is_not_found() {
    let core = core();
    let err = core
        .create_label(
            "ghost",
            "void",
            CreateLabelRequest {
                name: "bug".to_string(),
                color: "ededed".to_string(),
                description: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

// ---------------------------------------------------------------------------
// Issues + milestones + comments
// ---------------------------------------------------------------------------

#[test]
fn issues_get_sequential_numbers_starting_at_one() {
    let core = core_with_repo("alice", "jeryu");
    let first = core
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "first".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    let second = core
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "second".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    // GitHub-shaped: `number`, monotonic, starts at 1.
    assert_eq!(first.number, 1);
    assert_eq!(second.number, 2);
    assert_eq!(first.state, IssueState::Open);
    assert!(first.pull_request.is_none());
    assert_eq!(first.comments, 0);
}

#[test]
fn issue_carries_labels_assignees_and_milestone() {
    let core = core_with_repo("alice", "jeryu");
    let issue = core
        .create_issue(
            "alice",
            "jeryu",
            "reporter",
            CreateIssueRequest {
                title: "tracked".to_string(),
                body: Some("details".to_string()),
                labels: vec!["bug".to_string(), "p1".to_string()],
                assignees: vec!["alice".to_string()],
                milestone: Some("v1.0".to_string()),
            },
        )
        .unwrap();
    assert_eq!(issue.labels, vec!["bug".to_string(), "p1".to_string()]);
    assert_eq!(issue.assignees, vec!["alice".to_string()]);
    assert_eq!(issue.milestone.as_deref(), Some("v1.0"));
    assert_eq!(issue.author, "reporter");
    // creating an issue auto-provisions the author user.
    assert!(core.get_user("reporter").is_ok());
}

#[test]
fn closing_an_issue_sets_closed_at_and_reopening_clears_it() {
    let core = core_with_repo("alice", "jeryu");
    let issue = core
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "lifecycle".to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(issue.closed_at.is_none());

    let closed = core
        .update_issue(
            "alice",
            "jeryu",
            issue.number,
            UpdateIssueRequest {
                state: Some(IssueState::Closed),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(closed.state, IssueState::Closed);
    assert!(closed.closed_at.is_some());

    let reopened = core
        .update_issue(
            "alice",
            "jeryu",
            issue.number,
            UpdateIssueRequest {
                state: Some(IssueState::Open),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(reopened.state, IssueState::Open);
    assert!(reopened.closed_at.is_none());
}

#[test]
fn list_issues_filters_by_state() {
    let core = core_with_repo("alice", "jeryu");
    for title in ["a", "b", "c"] {
        core.create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: title.to_string(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    // Close the middle one.
    core.update_issue(
        "alice",
        "jeryu",
        2,
        UpdateIssueRequest {
            state: Some(IssueState::Closed),
            ..Default::default()
        },
    )
    .unwrap();

    let open = core
        .list_issues("alice", "jeryu", Some(IssueState::Open))
        .unwrap();
    assert_eq!(open.len(), 2);
    assert!(open.iter().all(|i| i.state == IssueState::Open));

    let closed = core
        .list_issues("alice", "jeryu", Some(IssueState::Closed))
        .unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].number, 2);

    let all = core.list_issues("alice", "jeryu", None).unwrap();
    assert_eq!(all.len(), 3);
    // returned in ascending number order.
    assert_eq!(
        all.iter().map(|i| i.number).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn empty_issue_title_is_rejected() {
    let core = core_with_repo("alice", "jeryu");
    let err = core
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "  ".to_string(),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn update_unknown_issue_is_not_found() {
    let core = core_with_repo("alice", "jeryu");
    let err = core
        .update_issue(
            "alice",
            "jeryu",
            999,
            UpdateIssueRequest {
                title: Some("x".to_string()),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

#[test]
fn comment_increments_count_and_lists() {
    let core = core_with_repo("alice", "jeryu");
    let issue = core
        .create_issue(
            "alice",
            "jeryu",
            "alice",
            CreateIssueRequest {
                title: "discuss".to_string(),
                ..Default::default()
            },
        )
        .unwrap();

    core.add_issue_comment(
        "alice",
        "jeryu",
        issue.number,
        "bob",
        CreateCommentRequest {
            body: "first".to_string(),
        },
    )
    .unwrap();
    let second = core
        .add_issue_comment(
            "alice",
            "jeryu",
            issue.number,
            "carol",
            CreateCommentRequest {
                body: "second".to_string(),
            },
        )
        .unwrap();

    assert_eq!(second.issue_number, issue.number);
    assert_eq!(second.author, "carol");

    let reread = core.get_issue("alice", "jeryu", issue.number).unwrap();
    assert_eq!(reread.comments, 2);

    let comments = core
        .list_issue_comments("alice", "jeryu", issue.number)
        .unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0].body, "first");
    assert_eq!(comments[1].body, "second");
}

#[test]
fn empty_comment_body_is_rejected() {
    let core = core_with_repo("alice", "jeryu");
    core.create_issue(
        "alice",
        "jeryu",
        "alice",
        CreateIssueRequest {
            title: "x".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    let err = core
        .add_issue_comment(
            "alice",
            "jeryu",
            1,
            "alice",
            CreateCommentRequest {
                body: String::new(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn comment_on_missing_issue_is_not_found() {
    let core = core_with_repo("alice", "jeryu");
    let err = core
        .add_issue_comment(
            "alice",
            "jeryu",
            42,
            "alice",
            CreateCommentRequest {
                body: "hi".to_string(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}
