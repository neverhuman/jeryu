//! Typed validation for the independent Jeryu split-family authority.

mod compliance;
mod identity;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::GateOutcome;
use compliance::{ComplianceContract, ComplianceState, validate_compliance};
use identity::{CONTROL_PLANE_PREDECESSOR_TAG, PRODUCT_AUTHORITIES};

/// Canonical authority-manifest path relative to the release control plane.
pub const FAMILY_MANIFEST_RELATIVE_PATH: &str = "repos.manifest.toml";
const SPLIT_ROOT: &str = "/home/ubuntu/jain-split/jeryu-split";
const AUTHORITY_PATH: &str =
    "/home/ubuntu/jain-split/jeryu-split/jeryu-release-ops/repos.manifest.toml";
const REDLINE_ROOT: &str = "/home/ubuntu/jain-split/jain-redline";
const LOCAL_FORGE_BASE_URL: &str = "http://127.0.0.1:8787";
const LOCAL_FORGE_GIT_TEMPLATE: &str = "http://127.0.0.1:8787/git/{owner}/{repo}.git";
const HOSTED_FORGE_BASE_URL: &str = "https://git.neverhuman.org";
const HOSTED_FORGE_GIT_TEMPLATE: &str = "https://git.neverhuman.org/git/{owner}/{repo}.git";
const HOSTED_DEPENDENCY_RESOLUTION: &str = "immutable-tags-via-hosted-transport";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    repo_family: String,
    release_identity: String,
    release_lineage: String,
    status: String,
    formal_ga: bool,
    split_root: String,
    manifest_authority: String,
    compliance_state: ComplianceState,
    forges: Forges,
    compliance: ComplianceContract,
    required_repos: Vec<String>,
    retired_histories: Vec<String>,
    control_plane: ControlPlane,
    nested_families: NestedFamilies,
    repo: Vec<Repository>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Forges {
    local_transition: ForgeProfile,
    hosted: ForgeProfile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ForgeProfile {
    provider: String,
    base_url: String,
    git_url_template: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NestedFamilies {
    redline: RedlineFamily,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RedlineFamily {
    family: String,
    consumer: String,
    source_authority: String,
    container_path: String,
    control_plane: String,
    manifest_path: String,
    dependency_resolution: String,
    required: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Repository {
    name: String,
    path: String,
    jeryu_slug: String,
    remote: String,
    required_check: String,
    default_branch: String,
    identity_status: IdentityStatus,
    current_tag: Option<String>,
    inventory_status: String,
    runtime_authority: String,
    product_name: Option<String>,
    lfs_required: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlPlane {
    authority_forge: String,
    name: String,
    path: String,
    jeryu_slug: String,
    remote: String,
    required_check: String,
    default_branch: String,
    identity_status: IdentityStatus,
    predecessor_tag: String,
    inventory_status: String,
    runtime_authority: String,
    product_name: Option<String>,
    lfs_required: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum IdentityStatus {
    Pending,
    Bound,
}

/// Validate the authority manifest rooted at `root`.
pub fn run_family_manifest(root: &Path) -> Result<GateOutcome> {
    let path = root.join(FAMILY_MANIFEST_RELATIVE_PATH);
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let manifest: Manifest =
        toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    validate(&manifest)?;
    Ok(GateOutcome {
        stdout: vec![format!(
            "Jeryu family authority ok: {} active checkouts, {} retired histories",
            manifest.repo.len() + 1,
            manifest.retired_histories.len()
        )],
        exit_code: 0,
    })
}

/// Validate a proposed one-way compliance-state transition between two authority manifests.
pub fn validate_family_manifest_transition(previous: &str, proposed: &str) -> Result<()> {
    let previous: Manifest =
        toml::from_str(previous).context("parse previous authority manifest")?;
    let proposed: Manifest =
        toml::from_str(proposed).context("parse proposed authority manifest")?;
    validate(&previous)?;
    validate(&proposed)?;
    ComplianceState::validate_transition(previous.compliance_state, proposed.compliance_state)
}

fn validate(manifest: &Manifest) -> Result<()> {
    if manifest.schema_version != "1"
        || manifest.repo_family != "jeryu-split"
        || manifest.release_identity != "jeryu-split"
        || manifest.release_lineage != "v5"
    {
        bail!("Jeryu family identity must remain independent jeryu-split with v5 lineage");
    }
    if manifest.status != "candidate" || manifest.formal_ga {
        bail!("Jeryu family authority must remain fail-closed candidate metadata");
    }
    if manifest.split_root != SPLIT_ROOT || manifest.manifest_authority != AUTHORITY_PATH {
        bail!("Jeryu split root or authority path is not canonical");
    }
    validate_forges(&manifest.forges)?;
    let authority_forge =
        selected_authority_forge(&manifest.forges, &manifest.control_plane.authority_forge)?;
    validate_compliance(&manifest.compliance)?;
    if manifest.control_plane.name != "jeryu-release-ops"
        || manifest.control_plane.runtime_authority != "control-plane"
        || manifest.control_plane.product_name.is_some()
    {
        bail!("Jeryu authority must use jeryu-release-ops as its control plane");
    }

    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut slugs = BTreeSet::new();
    let mut remotes = BTreeSet::new();
    for repo in &manifest.repo {
        let expected = PRODUCT_AUTHORITIES
            .iter()
            .find(|expected| expected.name == repo.name)
            .ok_or_else(|| anyhow::anyhow!("{} is not a governed Jeryu repository", repo.name))?;
        if repo.runtime_authority != expected.runtime
            || repo.product_name.as_deref() != expected.product_name
        {
            bail!("{} has the wrong runtime or product identity", repo.name);
        }
        validate_repository(repo, expected.tag, expected.lfs_required, authority_forge)?;
    }
    validate_control_plane(&manifest.control_plane, authority_forge)?;
    for repo in &manifest.repo {
        if !names.insert(repo.name.clone())
            || !paths.insert(repo.path.clone())
            || !slugs.insert(repo.jeryu_slug.clone())
            || !remotes.insert(repo.remote.clone())
        {
            bail!(
                "Jeryu authority contains a duplicate repository identity, path, slug, or origin"
            );
        }
    }
    let control = &manifest.control_plane;
    if !names.insert(control.name.clone())
        || !paths.insert(control.path.clone())
        || !slugs.insert(control.jeryu_slug.clone())
        || !remotes.insert(control.remote.clone())
    {
        bail!("Jeryu authority contains a duplicate repository identity, path, slug, or origin");
    }
    let retired = manifest
        .retired_histories
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if retired.len() != manifest.retired_histories.len()
        || (!retired.is_empty() && retired != BTreeSet::from(["jeryu-web".to_owned()]))
    {
        bail!("retired_histories may contain only the completed jeryu-web retirement");
    }

    let mut expected_names = PRODUCT_AUTHORITIES
        .iter()
        .map(|expected| expected.name.to_owned())
        .collect::<BTreeSet<_>>();
    if retired.contains("jeryu-web") {
        expected_names.remove("jeryu-web");
    }
    let product_names = manifest
        .repo
        .iter()
        .map(|repo| repo.name.clone())
        .collect::<BTreeSet<_>>();
    if product_names.len() != manifest.repo.len() || product_names != expected_names {
        bail!("Jeryu authority product rows do not match the governed family inventory");
    }

    let required = manifest
        .required_repos
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if required.len() != manifest.required_repos.len() || required != names {
        bail!("required_repos must exactly match active product and control-plane identities");
    }
    if retired.iter().any(|retired| names.contains(retired)) {
        bail!("an active repository cannot also be a retired history");
    }

    let redline = &manifest.nested_families.redline;
    if redline.family != "redline-split"
        || redline.consumer != "jeryu-split"
        || redline.source_authority != "jain-redline"
        || redline.container_path != REDLINE_ROOT
        || redline.control_plane != format!("{REDLINE_ROOT}/redline-split-ops")
        || redline.manifest_path != format!("{REDLINE_ROOT}/redline-split-ops/repos.manifest.toml")
        || redline.dependency_resolution != HOSTED_DEPENDENCY_RESOLUTION
        || !redline.required
    {
        bail!("Redline dependency must resolve only through canonical jain-redline authority");
    }
    Ok(())
}

fn validate_forges(forges: &Forges) -> Result<()> {
    validate_forge_profile(
        "local_transition",
        &forges.local_transition,
        LOCAL_FORGE_BASE_URL,
        LOCAL_FORGE_GIT_TEMPLATE,
    )?;
    validate_forge_profile(
        "hosted",
        &forges.hosted,
        HOSTED_FORGE_BASE_URL,
        HOSTED_FORGE_GIT_TEMPLATE,
    )
}

fn selected_authority_forge<'a>(forges: &'a Forges, selector: &str) -> Result<&'a ForgeProfile> {
    match selector {
        "hosted" => Ok(&forges.hosted),
        "local_transition" => {
            bail!("the local-transition forge is retained for compatibility, not authority")
        }
        _ => bail!("the Jeryu authority forge selector is unknown"),
    }
}

fn validate_forge_profile(
    name: &str,
    profile: &ForgeProfile,
    expected_base_url: &str,
    expected_git_template: &str,
) -> Result<()> {
    if profile.provider != "jeryu"
        || profile.base_url != expected_base_url
        || profile.git_url_template != expected_git_template
    {
        bail!("Jeryu forge profile {name} is not the governed transport identity");
    }
    Ok(())
}

fn validate_repository(
    repo: &Repository,
    expected_tag: Option<&str>,
    expected_lfs: bool,
    authority_forge: &ForgeProfile,
) -> Result<()> {
    let expected_path = PathBuf::from(SPLIT_ROOT).join(&repo.name);
    let expected_slug = format!("jeryu/{}", repo.name);
    let expected_remote = authority_forge
        .git_url_template
        .replace("{owner}", "jeryu")
        .replace("{repo}", &repo.name);
    let expected_check = format!("{}/required", repo.name);
    if repo.path != expected_path.to_string_lossy()
        || repo.jeryu_slug != expected_slug
        || repo.remote != expected_remote
        || repo.required_check != expected_check
        || repo.default_branch != "main"
        || repo.inventory_status != "active"
        || repo.lfs_required != expected_lfs
    {
        bail!(
            "{} has noncanonical path, forge identity, check, inventory, or LFS status",
            repo.name
        );
    }
    let current_tag = match (
        repo.identity_status,
        repo.current_tag.as_deref(),
        expected_tag,
    ) {
        (IdentityStatus::Pending, None, None) => return Ok(()),
        (IdentityStatus::Bound, Some(actual), Some(expected)) if actual == expected => actual,
        (IdentityStatus::Pending, Some(_), _) => {
            bail!("{} is pending but carries a bound release tag", repo.name)
        }
        (IdentityStatus::Bound, None, _) => {
            bail!("{} is bound without a release tag", repo.name)
        }
        (_, _, _) => bail!("{} release identity does not match authority", repo.name),
    };
    validate_tag_lineage(&repo.name, current_tag)
}

fn validate_control_plane(control: &ControlPlane, authority_forge: &ForgeProfile) -> Result<()> {
    let expected_path = PathBuf::from(SPLIT_ROOT).join(&control.name);
    let expected_slug = format!("jeryu/{}", control.name);
    let expected_remote = authority_forge
        .git_url_template
        .replace("{owner}", "jeryu")
        .replace("{repo}", &control.name);
    let expected_check = format!("{}/required", control.name);
    if control.name != "jeryu-release-ops"
        || control.authority_forge != "hosted"
        || control.path != expected_path.to_string_lossy()
        || control.jeryu_slug != expected_slug
        || control.remote != expected_remote
        || control.required_check != expected_check
        || control.default_branch != "main"
        || control.identity_status != IdentityStatus::Bound
        || control.predecessor_tag != CONTROL_PLANE_PREDECESSOR_TAG
        || control.inventory_status != "active"
        || control.runtime_authority != "control-plane"
        || control.product_name.is_some()
        || control.lfs_required
    {
        bail!("Jeryu release control plane has a noncanonical identity or predecessor tag");
    }
    validate_tag_lineage(&control.name, &control.predecessor_tag)
}

fn validate_tag_lineage(name: &str, tag: &str) -> Result<()> {
    let tag_prefix = format!("{name}-v5.");
    let Some(version_and_revision) = tag.strip_prefix(&tag_prefix) else {
        bail!("{name} must retain its immutable v5 split-tag lineage");
    };
    let Some((version, revision)) = version_and_revision.split_once("-split.") else {
        bail!("{name} must retain its immutable v5 split-tag lineage");
    };
    if version.split('.').count() != 2
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        || revision.is_empty()
        || !revision.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("{name} must retain its immutable v5 split-tag lineage");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
