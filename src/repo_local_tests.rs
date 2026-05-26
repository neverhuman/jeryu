use super::*;

fn config() -> LocalRepoConfig {
    LocalRepoConfig {
        source_path: PathBuf::from("root-veox.toml"),
        repo: "root/veox".into(),
        default_branch: "main".into(),
        shadow_main: ShadowMainConfig {
            enabled: true,
            remote_url: "git@github.com:neverhuman/warp.git".into(),
            refs: vec!["refs/heads/main".into()],
            trigger: "main_pipeline_success".into(),
            fallback_review: true,
        },
        backup: BackupConfig {
            target: "xbabe3:/home/ubuntu/jeryu-backups/veox".into(),
        },
    }
}

#[test]
fn parses_local_repo_config_shape() {
    let mut parsed: LocalRepoConfig = toml::from_str(
        r#"
repo = "root/veox"
default_branch = "main"

[shadow_main]
enabled = true
remote_url = "git@github.com:neverhuman/warp.git"
refs = ["refs/heads/main"]
trigger = "main_pipeline_success"
fallback_review = true

[backup]
target = "xbabe3:/home/ubuntu/jeryu-backups/veox"
"#,
    )
    .unwrap();
    parsed.source_path = PathBuf::from("root-veox.toml");
    assert_eq!(parsed.repo, "root/veox");
    assert!(parsed.shadow_main.enabled);
    assert_eq!(shadow_trigger(&parsed), "main_pipeline_success");
    assert!(parsed.shadow_main.fallback_review);
    assert_eq!(shadow_refs(&parsed), vec!["refs/heads/main"]);
    assert!(matches!(
        parse_backup_target(&parsed.backup.target).unwrap(),
        BackupTarget::Remote { .. }
    ));
}

#[test]
fn shadow_trigger_defaults_to_push_for_existing_configs() {
    let mut config = config();
    config.shadow_main.trigger.clear();
    assert_eq!(shadow_trigger(&config), "push");
}

#[test]
fn shadow_ref_matching_accepts_raw_or_full_refs() {
    let config = config();
    assert!(shadow_ref_matches(&config, "main"));
    assert!(shadow_ref_matches(&config, "refs/heads/main"));
    assert!(!shadow_ref_matches(&config, "refs/heads/feature"));
}

#[test]
fn repo_identifiers_are_safe_for_mirror_paths() {
    assert_eq!(safe_repo_component("root/veox"), "root-veox");
    assert_eq!(safe_repo_component("team/redlineDB"), "team-redlineDB");
}

#[test]
fn review_fallback_parses_github_remotes() {
    assert_eq!(
        parse_github_repo("https://github.com/neverhuman/jekko.git").as_deref(),
        Some("neverhuman/jekko")
    );
    assert_eq!(
        parse_github_repo("git@github.com:neverhuman/jekko.git").as_deref(),
        Some("neverhuman/jekko")
    );
}

#[test]
fn review_fallback_parses_local_gitlab_remotes() {
    assert_eq!(
        parse_gitlab_repo("ssh://git@127.0.0.1:2224/root/jekko.git").as_deref(),
        Some("root/jekko")
    );
}
