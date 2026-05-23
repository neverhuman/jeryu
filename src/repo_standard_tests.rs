use super::repo_standard_git::parse_remote_slug;
use super::*;
use std::fs;

#[test]
fn parse_remote_slug_accepts_common_git_remotes() {
    assert_eq!(
        parse_remote_slug("git@github.com:neverhuman/warp.git").as_deref(),
        Some("neverhuman/warp")
    );
    assert_eq!(
        parse_remote_slug("https://github.com/neverhuman/warp.git").as_deref(),
        Some("neverhuman/warp")
    );
}

#[test]
fn render_keeps_managed_policy_under_jeryu_except_host_integrations() {
    let spec = StandardSpec {
        repo_root: PathBuf::from("."),
        profile: "sovereign_plus".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: "neverhuman/warp".to_string(),
        repo_owner: "neverhuman".to_string(),
        repo_name: "warp".to_string(),
        autonomy_dir: DEFAULT_AUTONOMY_DIR.to_string(),
    };
    let files = render_standard_files(&spec);
    assert!(files.iter().any(|file| file.path == ".jeryu/standard.lock"));
    for file in files {
        assert!(
            file.path.starts_with(".jeryu/")
                || file.path.starts_with(".github/")
                || file.path == ".gitlab-ci.yml",
            "unexpected managed path outside .jeryu/host integration: {}",
            file.path
        );
    }
}

#[test]
fn gitlab_provider_renders_no_github_files() {
    let spec = StandardSpec {
        repo_root: PathBuf::from("."),
        profile: "sovereign_plus".to_string(),
        provider: StandardProvider::Gitlab,
        base_branch: "main".to_string(),
        repo_slug: "root/veox".to_string(),
        repo_owner: "root".to_string(),
        repo_name: "veox".to_string(),
        autonomy_dir: DEFAULT_AUTONOMY_DIR.to_string(),
    };
    let files = render_standard_files(&spec);
    assert!(files.iter().any(|file| file.path == ".gitlab-ci.yml"));
    assert!(files.iter().all(|file| !file.path.starts_with(".github/")));

    let delivery = files
        .iter()
        .find(|file| file.path == ".jeryu/delivery.toml")
        .unwrap();
    assert!(delivery.content.contains("github_actions_required = false"));
    assert!(delivery.content.contains("local_gitlab_required = true"));

    let protected = files
        .iter()
        .find(|file| file.path == ".jeryu/protected-paths.toml")
        .unwrap();
    assert!(!protected.content.contains(".github"));

    let lock = files
        .iter()
        .find(|file| file.path == ".jeryu/standard.lock")
        .unwrap();
    assert!(!lock.content.contains(".github/"));
}

#[test]
fn node_frontend_profile_renders_node_fast_lane() {
    let spec = StandardSpec {
        repo_root: PathBuf::from("."),
        profile: "node-frontend".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: "neverhuman/veox-warp".to_string(),
        repo_owner: "neverhuman".to_string(),
        repo_name: "veox-warp".to_string(),
        autonomy_dir: DEFAULT_AUTONOMY_DIR.to_string(),
    };
    let files = render_standard_files(&spec);
    let fast = files
        .iter()
        .find(|file| file.path == ".jeryu/ci/fast.sh")
        .unwrap();
    assert!(fast.content.contains("package.json"));
    assert!(fast.content.contains("npm run typecheck"));
    assert!(!fast.content.contains("Cargo.toml is required"));
}

#[test]
fn data_client_profile_renders_nested_manifest_fast_lane() {
    let spec = StandardSpec {
        repo_root: PathBuf::from("."),
        profile: "data-client".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: "neverhuman/veox-neverhuman-data".to_string(),
        repo_owner: "neverhuman".to_string(),
        repo_name: "veox-neverhuman-data".to_string(),
        autonomy_dir: DEFAULT_AUTONOMY_DIR.to_string(),
    };
    let files = render_standard_files(&spec);
    let fast = files
        .iter()
        .find(|file| file.path == ".jeryu/ci/fast.sh")
        .unwrap();
    assert!(fast.content.contains("crates/neverhuman-data/Cargo.toml"));
    assert!(fast.content.contains("cargo metadata --manifest-path"));
    assert!(!fast.content.contains("cargo check --workspace"));
}

#[test]
fn apply_then_verify_is_clean_in_temp_git_repo() {
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init", "-b", "main"]).unwrap();
    run_git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:neverhuman/warp.git",
        ],
    )
    .unwrap();

    let opts = RepoStandardOptions {
        path: tmp.path().to_path_buf(),
        profile: "sovereign_plus".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: None,
        autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
        configure_git_hooks: true,
        json: true,
    };

    assert_eq!(
        run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(),
        0
    );
    assert_eq!(run_standard(RepoStandardMode::Verify, opts).unwrap(), 0);
    assert!(tmp.path().join(".jeryu/project.toml").is_file());
    assert!(tmp.path().join(".jeryu/standard.lock").is_file());
    assert_eq!(
        git_config_get(tmp.path(), "core.hooksPath")
            .unwrap()
            .as_deref(),
        Some(".jeryu/hooks")
    );
}

#[test]
fn root_autonomy_tree_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".autonomy/policies")).unwrap();
    let opts = RepoStandardOptions {
        path: tmp.path().to_path_buf(),
        profile: "sovereign_plus".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: Some("neverhuman/warp".to_string()),
        autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
        configure_git_hooks: false,
        json: true,
    };

    let err = build_spec(&opts).unwrap_err();
    assert!(
        err.to_string().contains("root .autonomy is forbidden"),
        "{err:?}"
    );
}

#[test]
fn veox_hard_switch_repo_infers_remote_slug_and_writes_jeryu_policy() {
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init", "-b", "main"]).unwrap();
    run_git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:neverhuman/warp.git",
        ],
    )
    .unwrap();

    fs::create_dir_all(tmp.path().join(".jeryu")).unwrap();
    fs::write(
        tmp.path().join(".jeryu/delivery.toml"),
        "schema_version = \"1\"\nrepo = \"stale-owner/stale-repo\"\n",
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join(".jeryu/autonomy/policies")).unwrap();
    fs::write(
        tmp.path().join(".jeryu/autonomy/autonomy.yml"),
        "schema: vibegate.autonomy.v1\npolicy_root: .jeryu/autonomy/policies\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join(".jeryu/autonomy/policies/release.yml"),
        "schema: vibegate.release.v1\nbuild:\n  build_once: false\n  require_sbom: false\n  require_slsa_provenance: false\n  require_artifact_signature: false\n  require_rollback_plan: false\n",
    )
    .unwrap();

    let opts = RepoStandardOptions {
        path: tmp.path().to_path_buf(),
        profile: "sovereign_plus".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: None,
        autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
        configure_git_hooks: false,
        json: true,
    };

    let spec = build_spec(&opts).unwrap();
    assert_eq!(spec.repo_slug, "neverhuman/warp");
    let files = render_standard_files(&spec);
    let delivery = files
        .iter()
        .find(|file| file.path == ".jeryu/delivery.toml")
        .unwrap();
    assert!(delivery.content.contains("repo = \"neverhuman/warp\""));
    let plan = plan_standard(&spec, &files, &opts).unwrap();
    assert_eq!(plan.repo_slug, "neverhuman/warp");
    assert_eq!(
        plan.changes
            .iter()
            .find(|change| change.path == ".jeryu/delivery.toml")
            .unwrap()
            .operation,
        ManagedFileOperation::Update
    );
    assert_eq!(
        run_standard(RepoStandardMode::Plan, opts.clone()).unwrap(),
        0
    );

    assert_eq!(
        run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(),
        0
    );
    let rendered_delivery = fs::read_to_string(tmp.path().join(".jeryu/delivery.toml")).unwrap();
    assert!(rendered_delivery.contains("repo = \"neverhuman/warp\""));
    assert!(!rendered_delivery.contains("stale-owner/stale-repo"));

    for path in [
        ".jeryu/autonomy/autonomy.yml",
        ".jeryu/autonomy/policies/approvals.yml",
        ".jeryu/autonomy/policies/protected-paths.yml",
        ".jeryu/autonomy/policies/release.yml",
        ".jeryu/autonomy/policies/risk.yml",
        ".github/AGENTS.md",
        ".github/CODEOWNERS",
        ".github/workflows/jeryu-required.yml",
    ] {
        assert!(tmp.path().join(path).is_file(), "missing {path}");
    }
    let rendered_autonomy =
        fs::read_to_string(tmp.path().join(".jeryu/autonomy/autonomy.yml")).unwrap();
    assert!(rendered_autonomy.contains("policy_root: .jeryu/autonomy/policies"));
    let rendered_release =
        fs::read_to_string(tmp.path().join(".jeryu/autonomy/policies/release.yml")).unwrap();
    assert!(rendered_release.contains("release_ready_receipts:"));
    assert!(rendered_release.contains("require_artifact_signature: true"));
    assert!(!rendered_release.contains("require_artifact_signature: false"));
    assert!(
        !fs::symlink_metadata(tmp.path().join(".jeryu/autonomy"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::read_to_string(tmp.path().join(".github/CODEOWNERS"))
            .unwrap()
            .contains("@neverhuman")
    );
    assert!(
        fs::read_to_string(tmp.path().join(".github/workflows/jeryu-required.yml"))
            .unwrap()
            .contains("jeryu/required")
    );

    let spec = build_spec(&opts).unwrap();
    let files = render_standard_files(&spec);
    let clean_plan = plan_standard(&spec, &files, &opts).unwrap();
    assert!(report_is_clean(&clean_plan));
    assert_eq!(
        run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(),
        0
    );
    assert_eq!(run_standard(RepoStandardMode::Verify, opts).unwrap(), 0);
}

#[test]
fn verify_reports_drift_when_managed_file_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let opts = RepoStandardOptions {
        path: tmp.path().to_path_buf(),
        profile: "sovereign_plus".to_string(),
        provider: StandardProvider::Github,
        base_branch: "main".to_string(),
        repo_slug: Some("neverhuman/warp".to_string()),
        autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
        configure_git_hooks: false,
        json: true,
    };

    assert_eq!(
        run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(),
        0
    );
    fs::write(tmp.path().join(".jeryu/project.toml"), "drift\n").unwrap();
    assert_eq!(run_standard(RepoStandardMode::Verify, opts).unwrap(), 1);
}
