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
    assert_eq!(parsed.shadow_main.trigger, "main_pipeline_success");
    assert!(parsed.shadow_main.fallback_review);
    assert_eq!(shadow_refs(&parsed), vec!["refs/heads/main"]);
    assert!(matches!(
        parse_backup_target(&parsed.backup.target).unwrap(),
        BackupTarget::Remote { .. }
    ));
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
