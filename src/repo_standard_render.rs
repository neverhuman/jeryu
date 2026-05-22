use super::{AGENT_FIRST_STANDARD_VERSION, REQUIRED_CHECK_NAME, StandardProvider, StandardSpec};

#[path = "repo_standard_render_templates.rs"]
mod repo_standard_render_templates;
pub(super) use repo_standard_render_templates::*;

#[path = "repo_standard_render_files.rs"]
mod render_files;
pub(crate) use render_files::render_standard_files;

fn render_project_toml(spec: &StandardSpec) -> String {
    format!(
        "schema_version = \"1\"\nstandard = \"agent-first-autonomous\"\nstandard_version = \"{}\"\nproject_id = \"{}\"\nname = \"{}\"\ndefault_branch = \"{}\"\nstate_backend = \"sqlite\"\ncache_policy = \"isolated\"\nmanaged_policy_root = \".jeryu\"\n",
        AGENT_FIRST_STANDARD_VERSION, spec.repo_slug, spec.repo_name, spec.base_branch
    )
}

fn render_delivery_toml(spec: &StandardSpec) -> String {
    let provider_controls = match spec.provider {
        StandardProvider::Github => {
            "github_actions_required = true\nactions_must_be_pinned_to_sha = true\njob_permissions_default = \"read-only\"\n".to_string()
        }
        StandardProvider::Gitlab => {
            "github_actions_required = false\nlocal_gitlab_required = true\ngitlab_required_job = \"jeryu/required\"\n".to_string()
        }
    };
    format!(
        "schema_version = \"1\"\nprofile = \"{}\"\nprovider = \"{}\"\nrepo = \"{}\"\nbase_branch = \"{}\"\nautonomy_dir = \"{}\"\nrequired_check = \"{}\"\nmerge_queue_required = true\nmain_is_only_release_branch = true\n{}deploy_identity = \"oidc\"\nlong_lived_deploy_credentials_allowed = false\n\n[artifact]\nbuild_once = true\npromote_same_digest = true\nrequire_signature = true\nrequire_sbom = true\nrequire_provenance = true\nrollback = \"previous_signed_digest\"\n\n[approvals]\ndefault_human_approvals = 0\nprotected_path_human_approvals = 1\ncommittee_approval_default = false\nagent_self_approval_allowed = false\n",
        spec.profile,
        spec.provider,
        spec.repo_slug,
        spec.base_branch,
        spec.autonomy_dir,
        REQUIRED_CHECK_NAME,
        provider_controls
    )
}

fn render_release_policy_toml(spec: &StandardSpec) -> String {
    format!(
        "schema_version = \"1\"\nbase_branch = \"{}\"\nrelease_branches_allowed = false\nenvironment_branches_allowed = false\nmanual_deploy_branches_allowed = false\nmerge_queue_required = true\nrequired_check = \"{}\"\n\n[build]\nsource = \"green-main\"\nonce = true\nrebuild_during_promotion = false\n\n[promotion]\nstages = [\"local\", \"dev-canary\", \"prod-limited\", \"prod-full\"]\nidentity = \"oidc\"\nverify_digest_each_stage = true\n\n[rollback]\nstrategy = \"redeploy-previous-signed-digest\"\nrebuild_allowed = false\n\n[migrations]\nstrategy = \"expand-deploy-contract\"\ncontract_overlap_release_count = 1\nsuperseded_read_paths_allowed = false\n",
        spec.base_branch, REQUIRED_CHECK_NAME
    )
}

fn render_risk_policy_toml() -> String {
    "schema_version = \"1\"\n\n[[tier]]\nname = \"R0\"\nhuman_approvals = 0\nagent_review_required = true\n\n[[tier]]\nname = \"R1\"\nhuman_approvals = 0\nagent_review_required = true\n\n[[tier]]\nname = \"R2\"\nhuman_approvals = 0\nagent_review_required = true\n\n[[tier]]\nname = \"R3\"\nhuman_approvals = 1\nagent_review_required = true\n\n[[tier]]\nname = \"R4\"\nhuman_approvals = 1\nagent_review_required = true\n\n[[tier]]\nname = \"R5\"\nhuman_approvals = 1\nbreak_glass_required = true\n".to_string()
}

fn render_protected_paths_toml(spec: &StandardSpec) -> String {
    let host_paths = match spec.provider {
        StandardProvider::Github => "  \".github/**\",\n  \".gitlab-ci.yml\",\n",
        StandardProvider::Gitlab => "  \".gitlab-ci.yml\",\n",
    };
    format!(
        "schema_version = \"1\"\nowner = \"@{}\"\nhuman_approvals = 1\npaths = [\n{}  \".jeryu/**\",\n  \"ops/ci/**\",\n  \"release.policy.toml\",\n  \"Cargo.lock\",\n]\n",
        spec.repo_owner, host_paths
    )
}

fn render_required_sh() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"
mkdir -p target/jankurai

if ! command -v jankurai >/dev/null 2>&1; then
  echo "jeryu required: installing pinned jankurai binary" >&2
  bash .jeryu/ci/install-jankurai.sh
fi

jankurai audit . \
  --changed-fast \
  --changed-from "${JERYU_CHANGED_FROM:-origin/main}" \
  --mode advisory \
  --json target/jankurai/required-audit.json \
  --md target/jankurai/required-audit.md

bash .jeryu/ci/fast.sh
"#
    .to_string()
}

fn render_fast_sh() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

if [ ! -f Cargo.toml ]; then
  echo "jeryu fast: Cargo.toml is required by this standard profile" >&2
  exit 1
fi

cargo check --workspace --locked
"#
    .to_string()
}

fn render_pre_push_hook(base_branch: &str, provider: StandardProvider) -> String {
    let merge_surface = match provider {
        StandardProvider::Github => "PR plus merge queue",
        StandardProvider::Gitlab => "merge request plus local JeRyu/GitLab gate",
    };
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

protected_branch="{base_branch}"
while read -r _local_ref _local_sha remote_ref _remote_sha; do
  if [ "$remote_ref" = "refs/heads/$protected_branch" ]; then
    echo "jeryu: direct push to $protected_branch is blocked; use {merge_surface}" >&2
    exit 1
  fi
done

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
bash .jeryu/ci/required.sh
"#
    )
}

fn render_pre_commit_hook() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
mkdir -p target/jankurai
if command -v jankurai >/dev/null 2>&1; then
  jankurai audit . \
    --changed-fast \
    --changed-from "${JERYU_CHANGED_FROM:-origin/main}" \
    --mode advisory \
    --json target/jankurai/pre-commit-audit.json \
    --md target/jankurai/pre-commit-audit.md
else
  echo "jeryu: jankurai is not installed; run bash .jeryu/ci/install-jankurai.sh" >&2
  exit 1
fi
"#
    .to_string()
}
