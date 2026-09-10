use super::*;

const FIRST: &str = "0123456789abcdef0123456789abcdef01234567";
const SECOND: &str = "1123456789abcdef0123456789abcdef01234567";

fn source(repository: &str, scope: &str, commit: Option<&str>, minimum: u8) -> Source {
    Source {
        repository: repository.into(),
        scope: scope.into(),
        path: None,
        commit: commit.map(str::to_owned),
        minimum,
        required: scope != "optional",
        reason: None,
    }
}

fn family() -> toml::Value {
    let names = [
        "jeryu",
        "jeryu-core",
        "jeryu-cache",
        "jeryu-ci-runner",
        "jeryu-intelligence",
        "jeryu-jira",
        "jeryu-web",
        "jeryu-tool",
        "jeryu-tool-finder",
        "jeryu-deploy",
        "jeryu-release-ops",
    ];
    let body: String = names
        .iter()
        .map(|name| {
            let path = if *name == "jeryu" {
                ".".into()
            } else {
                format!("components/{name}")
            };
            format!(
                "[[repo]]\nname = {name:?}\ngithub_slug = \"neverhuman/{name}\"\npath = {path:?}\n"
            )
        })
        .collect();
    toml::from_str(&body).unwrap()
}

fn inventory(schema: &str, mut additions: Vec<Source>) -> Inventory {
    additions.insert(
        0,
        source("neverhuman/jankurai", "dependency", Some(FIRST), 85),
    );
    Inventory {
        schema: schema.into(),
        sources: additions,
    }
}

#[test]
fn both_external_revisions_are_selected_for_distinct_census_rows() {
    let selected = sources(
        &family(),
        inventory(
            "jeryu.audit-repositories/v2",
            vec![
                source("neverhuman/support", "optional", Some(FIRST), 95),
                source("neverhuman/support", "optional", Some(SECOND), 85),
            ],
        ),
    )
    .unwrap();
    let rows: Vec<_> = selected
        .iter()
        .filter(|source| source.repository == "neverhuman/support")
        .map(|source| serde_json::to_value(source).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["commit"], FIRST);
    assert_eq!(rows[0]["minimum"], 95);
    assert_eq!(rows[1]["commit"], SECOND);
    assert_eq!(rows[1]["minimum"], 85);
    assert!(rows.iter().all(|row| row["required"] == false));
}

#[test]
fn exact_duplicates_are_rejected_and_legacy_uniqueness_is_preserved() {
    for schema in ["jeryu.audit-repositories/v1", "jeryu.audit-repositories/v2"] {
        assert!(
            sources(
                &family(),
                inventory(
                    schema,
                    vec![
                        source("neverhuman/support", "optional", Some(FIRST), 95),
                        source("neverhuman/support", "optional", Some(FIRST), 85),
                    ]
                )
            )
            .is_err()
        );
    }
    assert!(
        sources(
            &family(),
            inventory(
                "jeryu.audit-repositories/v1",
                vec![
                    source("neverhuman/support", "optional", Some(FIRST), 85),
                    source("neverhuman/support", "optional", Some(SECOND), 85),
                ]
            )
        )
        .is_err()
    );
}

#[test]
fn missing_and_invalid_external_commits_cannot_be_enrolled_as_real_sources() {
    for commit in [
        None,
        Some("main"),
        Some(""),
        Some("ABCDEF0123456789abcdef0123456789abcdef0123"),
    ] {
        for scope in ["dependency", "optional"] {
            assert!(
                identity(
                    &source("neverhuman/support", scope, commit, 85),
                    Version::Versioned
                )
                .is_err()
            );
        }
    }
    let mut unresolved = source("unresolved/producer", "optional", None, 85);
    assert!(identity(&unresolved, Version::Versioned).is_err());
    unresolved.reason = Some("owner must identify an immutable public source".into());
    assert!(identity(&unresolved, Version::Versioned).is_ok());
    unresolved.required = true;
    assert!(identity(&unresolved, Version::Versioned).is_err());
}

#[test]
fn source_forms_and_standalone_acquisition_overlays_remain_isolated() {
    let same_revision = [
        source("neverhuman/support", "dependency", Some(FIRST), 85),
        source("neverhuman/support", "optional", Some(FIRST), 85),
    ];
    assert_ne!(
        identity(&same_revision[0], Version::Versioned).unwrap(),
        identity(&same_revision[1], Version::Versioned).unwrap()
    );
    for scope in ["monorepo", "component"] {
        let repository = if scope == "monorepo" {
            "neverhuman/jeryu"
        } else {
            "neverhuman/jeryu-core"
        };
        assert!(
            sources(
                &family(),
                inventory(
                    "jeryu.audit-repositories/v2",
                    vec![source(repository, scope, Some(FIRST), 85),]
                )
            )
            .is_err()
        );
    }
    for (commit, minimum, required) in [
        (None, 91, true),
        (Some(FIRST), 85, true),
        (Some(FIRST), 91, false),
    ] {
        let mut mirror = source("neverhuman/jeryu-cache", "standalone", commit, minimum);
        mirror.required = required;
        assert!(
            sources(
                &family(),
                inventory("jeryu.audit-repositories/v2", vec![mirror])
            )
            .is_err()
        );
    }
    let selected = sources(
        &family(),
        inventory(
            "jeryu.audit-repositories/v2",
            vec![source(
                "neverhuman/jeryu-cache",
                "standalone",
                Some(FIRST),
                95,
            )],
        ),
    )
    .unwrap();
    let mirror = selected
        .iter()
        .find(|source| {
            source.repository == "neverhuman/jeryu-cache" && source.scope == "standalone"
        })
        .unwrap();
    assert_eq!(mirror.minimum, 95);
    assert_eq!(mirror.commit.as_deref(), Some(FIRST));
    assert!(
        sources(
            &family(),
            inventory(
                "jeryu.audit-repositories/v2",
                vec![
                    source("neverhuman/jeryu-cache", "standalone", Some(FIRST), 91),
                    source("neverhuman/jeryu-cache", "standalone", Some(SECOND), 91),
                ]
            )
        )
        .is_err()
    );
}
