use super::*;

// Preserve the released split-authority validator and its hostile cases during
// handover. The candidate monorepo schema is validated by jeryu-split.
const RAW: &str = include_str!("fixtures/split-v5.toml");

fn canonical() -> Manifest {
    toml::from_str(RAW).expect("canonical manifest parses")
}

#[test]
fn canonical_authority_passes() {
    validate(&canonical()).unwrap();
}

#[test]
fn hosted_authority_selector_and_profiles_are_explicit_and_fail_closed() {
    let manifest = canonical();
    assert_eq!(manifest.control_plane.authority_forge, "hosted");
    assert_eq!(manifest.forges.local_transition.provider, "jeryu");
    assert_eq!(manifest.forges.hosted.provider, "jeryu");
    assert_eq!(
        manifest.control_plane.remote,
        "https://git.neverhuman.org/git/jeryu/jeryu-release-ops.git"
    );
    for repo in &manifest.repo {
        assert_eq!(
            repo.remote,
            format!("https://git.neverhuman.org/git/jeryu/{}.git", repo.name)
        );
    }

    let mut implicit_loopback = canonical();
    implicit_loopback.forges.local_transition.base_url.clear();
    assert!(validate(&implicit_loopback).is_err());

    let mut insecure_hosted = canonical();
    insecure_hosted.forges.hosted.base_url = "http://git.neverhuman.org".to_owned();
    assert!(validate(&insecure_hosted).is_err());

    let mut malformed_template = canonical();
    malformed_template.forges.hosted.git_url_template =
        "https://git.neverhuman.org/git/{repo}/{owner}.git".to_owned();
    assert!(validate(&malformed_template).is_err());

    let mut unsupported_provider = canonical();
    unsupported_provider.forges.hosted.provider = "github".to_owned();
    assert!(validate(&unsupported_provider).is_err());

    let mut authority_rollback = canonical();
    authority_rollback.control_plane.authority_forge = "local_transition".to_owned();
    assert!(validate(&authority_rollback).is_err());

    let mut unknown_authority = canonical();
    unknown_authority.control_plane.authority_forge = "unknown".to_owned();
    assert!(validate(&unknown_authority).is_err());

    let mut loopback_product = canonical();
    loopback_product.repo[0].remote = "http://127.0.0.1:8787/git/jeryu/jeryu.git".to_owned();
    assert!(validate(&loopback_product).is_err());

    let mut loopback_control = canonical();
    loopback_control.control_plane.remote =
        "http://127.0.0.1:8787/git/jeryu/jeryu-release-ops.git".to_owned();
    assert!(validate(&loopback_control).is_err());

    let mut loopback_redline_transport = canonical();
    loopback_redline_transport
        .nested_families
        .redline
        .dependency_resolution = "immutable-local-forge-tags".to_owned();
    assert!(validate(&loopback_redline_transport).is_err());

    let unknown_profile = RAW.replace(
        "[forges.hosted]",
        "[forges.unknown]\nprovider = \"jeryu\"\nbase_url = \"https://unknown.invalid\"\ngit_url_template = \"https://unknown.invalid/git/{owner}/{repo}.git\"\n\n[forges.hosted]",
    );
    assert!(toml::from_str::<Manifest>(&unknown_profile).is_err());
}

#[test]
fn updated_release_identities_are_exact() {
    for (name, hostile) in [
        ("jeryu-cache", "jeryu-cache-v5.0.0-split.0"),
        ("jeryu-cache", "jeryu-cache-v5.0.0-split.2"),
        ("jeryu-core", "jeryu-core-v5.0.0-split.3"),
        ("jeryu-core", "jeryu-core-v5.0.0-split.4"),
        ("jeryu-core", "jeryu-core-v5.0.0-split.6"),
        ("jeryu-deploy", "jeryu-deploy-v5.0.0-split.2"),
        ("jeryu-deploy", "jeryu-deploy-v5.0.0-split.4"),
        ("jeryu-tool", "jeryu-tool-v5.1.0-split.3"),
        ("jeryu-tool", "jeryu-tool-v5.1.0-split.6"),
        ("jeryu-tool", "jeryu-tool-v5.1.0-split.8"),
        ("jeryu-web", "jeryu-web-v5.0.0-split.0"),
        ("jeryu-web", "jeryu-web-v5.0.0-split.2"),
    ] {
        let mut manifest = canonical();
        manifest
            .repo
            .iter_mut()
            .find(|repo| repo.name == name)
            .unwrap()
            .current_tag = Some(hostile.to_owned());
        assert!(validate(&manifest).is_err());
    }
}

#[test]
fn rejects_old_root_duplicate_redline_and_slug_alias() {
    for hostile in [
        RAW.replace(SPLIT_ROOT, "/home/ubuntu/jeryu-split"),
        RAW.replace(
            REDLINE_ROOT,
            "/home/ubuntu/jain-split/jeryu-split/jeryu-redline",
        ),
        RAW.replacen("jeryu/jeryu-cache", "veox/jeryu-cache", 1),
    ] {
        let manifest: Manifest = toml::from_str(&hostile).unwrap();
        assert!(validate(&manifest).is_err());
    }
}

#[test]
fn rejects_duplicate_and_retired_active_identity() {
    let mut duplicate = canonical();
    duplicate.repo[1].path = duplicate.repo[0].path.clone();
    assert!(validate(&duplicate).is_err());

    let mut retired = canonical();
    retired.retired_histories.push("jeryu-web".to_owned());
    assert!(validate(&retired).is_err());
}

#[test]
fn rejects_unknown_fields_and_self_consistent_identity_substitution() {
    assert!(toml::from_str::<Manifest>(&format!("unknown = true\n{RAW}")).is_err());
    assert!(toml::from_str::<Manifest>(&format!("{RAW}\nunknown = true\n")).is_err());

    let mut substituted = canonical();
    let repo = &mut substituted.repo[0];
    repo.name = "jeryu-unknown".to_owned();
    repo.path = format!("{SPLIT_ROOT}/jeryu-unknown");
    repo.jeryu_slug = "jeryu/jeryu-unknown".to_owned();
    repo.remote = "https://git.neverhuman.org/git/jeryu/jeryu-unknown.git".to_owned();
    repo.required_check = "jeryu-unknown/required".to_owned();
    repo.current_tag = Some("jeryu-unknown-v5.0.0-split.0".to_owned());
    substituted.required_repos[0] = "jeryu-unknown".to_owned();
    assert!(validate(&substituted).is_err());
}

#[test]
fn accepts_only_the_governed_web_retirement_transition() {
    let mut retired = canonical();
    retired.repo.retain(|repo| repo.name != "jeryu-web");
    retired.required_repos.retain(|repo| repo != "jeryu-web");
    retired.retired_histories.push("jeryu-web".to_owned());
    validate(&retired).unwrap();

    retired.retired_histories[0] = "jeryu-core".to_owned();
    assert!(validate(&retired).is_err());
}

#[test]
fn rejects_control_plane_work_and_malformed_tag_identity() {
    let mut control = canonical();
    control.control_plane.product_name = Some("Work".to_owned());
    assert!(validate(&control).is_err());

    let mut work = canonical();
    work.repo
        .iter_mut()
        .find(|repo| repo.name == "jeryu-jira")
        .unwrap()
        .product_name = Some("Jira".to_owned());
    assert!(validate(&work).is_err());

    let mut tag = canonical();
    tag.repo[0].current_tag = Some("jeryu-v5.latest-split.next".to_owned());
    assert!(validate(&tag).is_err());
}

#[test]
fn rejects_stale_or_invented_control_plane_tags() {
    for hostile in [
        "jeryu-release-ops-v5.0.0-split.0",
        "jeryu-release-ops-v5.0.0-split.1",
        "jeryu-release-ops-v5.0.0-split.2",
        "jeryu-release-ops-v5.0.0-split.3",
        "jeryu-release-ops-v5.0.0-split.4",
        "jeryu-release-ops-v5.0.0-split.5",
        "jeryu-release-ops-v5.0.0-split.7",
    ] {
        let mut manifest = canonical();
        manifest.control_plane.predecessor_tag = hostile.to_owned();
        assert!(validate(&manifest).is_err());
    }
}

#[test]
fn control_plane_and_product_tag_roles_cannot_be_mixed() {
    assert!(
        toml::from_str::<Manifest>(&RAW.replacen("predecessor_tag", "current_tag", 1)).is_err()
    );
    assert!(
        toml::from_str::<Manifest>(&RAW.replacen(
            "current_tag = \"jeryu-v5.0.0-split.0\"",
            "predecessor_tag = \"jeryu-v5.0.0-split.0\"",
            1,
        ))
        .is_err()
    );
}

#[test]
fn pending_and_bound_release_identities_are_fail_closed() {
    let mut pending_with_tag = canonical();
    let finder = pending_with_tag
        .repo
        .iter_mut()
        .find(|repo| repo.name == "jeryu-tool-finder")
        .unwrap();
    finder.current_tag = Some("jeryu-tool-finder-v5.1.0-split.0".to_owned());
    assert!(validate(&pending_with_tag).is_err());

    let mut bound_without_tag = canonical();
    bound_without_tag.repo[0].current_tag = None;
    assert!(validate(&bound_without_tag).is_err());

    let mut invented_bound = canonical();
    invented_bound.repo[0].current_tag = Some("jeryu-v5.0.0-split.9".to_owned());
    assert!(validate(&invented_bound).is_err());

    assert!(
        toml::from_str::<Manifest>(&RAW.replacen(
            "identity_status = \"bound\"",
            "identity_status = \"released\"",
            1,
        ))
        .is_err()
    );
}

#[test]
fn rejects_weakened_compliance_contract() {
    let mut hostiles = Vec::new();
    for (from, to) in [
        ("minimum_jankurai_score = 85", "minimum_jankurai_score = 84"),
        ("max_authored_lines = 500", "max_authored_lines = 501"),
        ("target_authored_lines = 300", "target_authored_lines = 301"),
        (
            "max_raw_git_blob_bytes = 1048576",
            "max_raw_git_blob_bytes = 1048577",
        ),
        ("max_caps = 0", "max_caps = 1"),
        ("max_hard_findings = 0", "max_hard_findings = 1"),
        ("max_soft_findings = 0", "max_soft_findings = 1"),
        ("max_disabled_rules = 0", "max_disabled_rules = 1"),
        ("allowed_score_drop = 0", "allowed_score_drop = 1"),
    ] {
        hostiles.push(RAW.replacen(from, to, 1));
    }
    for hostile in hostiles {
        let manifest: Manifest = toml::from_str(&hostile).unwrap();
        assert!(validate(&manifest).is_err());
    }
}

#[test]
fn requires_explicit_lfs_declarations_and_closed_states() {
    let missing_lfs = RAW.replacen("lfs_required = false\n", "", 1);
    assert!(toml::from_str::<Manifest>(&missing_lfs).is_err());
    let missing_requirement = RAW.replacen("max_soft_findings = 0\n", "", 1);
    assert!(toml::from_str::<Manifest>(&missing_requirement).is_err());
    assert!(
        toml::from_str::<Manifest>(&RAW.replace(
            "compliance_state = \"inventory\"",
            "compliance_state = \"disabled\"",
        ))
        .is_err()
    );

    let mut changed = canonical();
    changed.repo[0].lfs_required = true;
    assert!(validate(&changed).is_err());
}

#[test]
fn compliance_state_is_one_way() {
    let enforced = RAW.replace(
        "compliance_state = \"inventory\"",
        "compliance_state = \"enforced\"",
    );
    validate_family_manifest_transition(RAW, &enforced).unwrap();
    validate_family_manifest_transition(&enforced, &enforced).unwrap();
    assert!(validate_family_manifest_transition(&enforced, RAW).is_err());
}
