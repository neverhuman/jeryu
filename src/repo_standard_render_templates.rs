use crate::repo_standard::sha256_hex;
use crate::repo_standard::{AGENT_FIRST_STANDARD_VERSION, ManagedFile, StandardSpec};

pub(crate) fn render_autonomy_yml(spec: &StandardSpec) -> String {
    format!(
        "schema: vibegate.autonomy.v1\ndefault_profile: {}\nautonomous_prod_promotion: true\npolicy_root: .jeryu/autonomy/policies\nprotected_path_policy: .jeryu/autonomy/policies/protected-paths.yml\nrelease_policy: .jeryu/autonomy/policies/release.yml\nkill_bell_required: true\nfreeze_windows_fail_closed: true\nshadow_agreement_minimum: 0.95\n",
        spec.profile
    )
}

pub(crate) fn render_autonomy_approvals_yml() -> String {
    "schema: vibegate.approvals.v1\ninvariants:\n  no_self_approval: true\n  exact_sha_required: true\n  target_branch_policy_only: true\n  fail_closed_on_missing_evidence: true\n  fail_closed_on_agent_disagreement: true\n  require_distinct_agent_identities: true\nhard_stops:\n  - name: secret_scan_failed\n  - name: sast_failed\n  - name: reviewer_blocked\n  - name: sha_drift\n  - name: policy_sha_drift\n  - name: missing_required_review_role\n  - name: missing_evidence_pack\n  - name: evidence_signature_invalid\n  - name: prompt_injection_suspected\n  - name: codeowners_not_satisfied\n  - name: freeze_window_active\n  - name: budget_exceeded\n  - name: training_use_required_but_disallowed\n  - name: lockfile_diff_without_manifest_diff\n  - name: judge_signature_invalid\nquorum:\n  R0: { approvals_needed: 0, roles: [], human_required: false }\n  R1: { approvals_needed: 1, roles: [test_integrity], human_required: false }\n  R2: { approvals_needed: 2, roles: [test_integrity, security], human_required: false }\n  R3:\n    approvals_needed: 4\n    roles: [test_integrity, security, runtime, lockfile]\n    human_required: true\n  R4: { approvals_needed: 0, roles: [], human_required: true, fail_closed_without_human: true }\n  R5: { approvals_needed: 0, roles: [], human_required: true, fail_closed: true }\nverdict_ttl_minutes: 60\nre_judge_on:\n  - merge_train_rebase\n  - target_branch_advance\n  - policy_change_on_target\n  - new_commit_on_pr\n".to_string()
}

pub(crate) fn render_autonomy_risk_yml() -> String {
    "schema: vibegate.risk.v1\ntiers:\n  - id: R5\n    description: \"missing/tampered evidence, suspicious behavior, emergency, unknown blast radius\"\n    matchers:\n      - conditions: [evidence_missing]\n      - conditions: [evidence_signature_invalid]\n      - conditions: [prompt_injection_suspected]\n      - conditions: [policy_sha_drift]\n    auto_merge: false\n    human_required: true\n    fail_closed: true\n  - id: R4\n    description: \"auth, crypto, secrets, infra, CI, policy, release, prod, prompt/judge rules\"\n    matchers:\n      - any_path_matches_protected: true\n      - conditions: [changes_security_scanner_config]\n      - conditions: [changes_release_or_deploy_policy]\n      - conditions: [changes_agent_prompts_or_judge_policy]\n      - conditions: [touches_secret_handling]\n      - conditions: [destructive_database_change]\n    auto_merge: false\n    human_required: true\n  - id: R3\n    description: \"large, novel, dependency, performance, data, or broad behavior change\"\n    matchers:\n      - conditions: [lockfile_only_change]\n      - conditions: [dependency_count_delta_gte_5]\n      - conditions: [removes_or_weakens_tests]\n      - conditions: [introduces_new_external_code_source]\n      - lines_changed_gte: 800\n      - paths_match: [\"**/migrations/**\"]\n    auto_merge: false\n    required_reviews: [test_integrity, security, runtime, lockfile]\n    human_required: true\n  - id: R0\n    description: \"docs, comments, formatting, harmless metadata\"\n    matchers:\n      - paths_only_in: [\"**/*.md\", \"docs/**\", \"**/*.txt\", \"**/*.rst\", \"**/*.adoc\"]\n      - paths_only_in: [\"**/COMMENTS\", \"**/*.gitignore\", \"**/*.editorconfig\"]\n    auto_merge: true\n    required_reviews: []\n  - id: R1\n    description: \"small isolated code change with strong targeted tests\"\n    matchers:\n      - lines_changed_lte: 60\n        all_files_have_targeted_tests: true\n    auto_merge: true\n    required_reviews: [test_integrity]\n  - id: R2\n    description: \"normal product change (default catch-all; checked last)\"\n    matchers:\n      - default: true\n    auto_merge: true\n    required_reviews: [test_integrity, security]\nevaluation_order: top_down\n".to_string()
}

pub(crate) fn render_autonomy_protected_paths_yml() -> String {
    "schema: vibegate.protected-paths.v1\nhard_human:\n  - .gitlab-ci.yml\n  - .jeryu/**\n  - ops/ci/**\n  - release.policy.toml\n  - Cargo.lock\nsemantic_triggers:\n  - auth_boundary\n  - secret_handling\n  - release_policy\n".to_string()
}

pub(crate) fn render_autonomy_release_yml(_spec: &StandardSpec) -> String {
    "schema: vibegate.release.v1\nbuild:\n  build_once: true\n  require_sbom: true\n  require_slsa_provenance: true\n  require_artifact_signature: true\n  require_rollback_plan: true\ncanary:\n  initial_percent: 1\n  max_percent_without_human: 10\n  analysis_minutes: 30\nrelease_ready_receipts:\n  - intake\n  - vti-plan\n  - proof-receipt\n  - risk-gate\n  - reviewer-agent\n  - rollback-plan\n  - ci-checks\n".to_string()
}

pub(crate) fn render_github_required_workflow() -> String {
    r#"name: jeryu required

on:
  pull_request:
  merge_group:

permissions:
  contents: read

jobs:
  required:
    name: jeryu/required
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6
        with:
          fetch-depth: 0
      - name: Required lane
        run: bash .jeryu/ci/required.sh
"#
    .to_string()
}

pub(crate) fn render_gitlab_ci_yml() -> String {
    r#"stages:
  - required

jeryu/required:
  stage: required
  script:
    - bash .jeryu/ci/required.sh
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
    - if: '$CI_COMMIT_BRANCH == "main"'
"#
    .to_string()
}

pub(crate) fn render_github_agents_md() -> String {
    r#"# .github/AGENTS.md

Read `AGENTS.md` first. Workflow files in this directory are adapters for the canonical ops lanes under `ops/ci/`.
Owns `.github/`.
Forbidden: product feature code, domain policy, and handwritten release bypasses.
Proof lane: `bash ops/ci/quality-gates.sh` plus the matching `ops/ci/*-lane.sh` script for the workflow being changed.
"#
    .to_string()
}

pub(crate) fn render_codeowners(spec: &StandardSpec) -> String {
    format!(
        "* @{}\n.github/** @{}\n.jeryu/** @{}\nops/ci/** @{}\n",
        spec.repo_owner, spec.repo_owner, spec.repo_owner, spec.repo_owner
    )
}

pub(crate) fn render_pr_template() -> String {
    r#"## Change Surface
- [ ] R0 docs/comment-only
- [ ] R1 small fix or test-only
- [ ] R2 normal product change
- [ ] R3 runtime, dependency, data, or API behavior
- [ ] R4 protected path: .github, .jeryu, release, security, CI
- [ ] R5 break-glass or production incident

## Evidence
- [ ] `jeryu/required` passed
- [ ] Independent agent review receipt attached or linked
- [ ] Rollback path unchanged or tested
- [ ] Same artifact digest is promoted across environments when release-bound
"#
    .to_string()
}

pub(crate) fn render_standard_lock(spec: &StandardSpec, files: &[ManagedFile]) -> String {
    let mut out = format!(
        "schema_version = \"1\"\nstandard_version = \"{}\"\nprovider = \"{}\"\nrepo = \"{}\"\nbase_branch = \"{}\"\nautonomy_dir = \"{}\"\n\n",
        AGENT_FIRST_STANDARD_VERSION,
        spec.provider,
        spec.repo_slug,
        spec.base_branch,
        spec.autonomy_dir
    );
    for file in files {
        out.push_str("[[managed_file]]\n");
        out.push_str(&format!("path = \"{}\"\n", file.path));
        out.push_str(&format!(
            "sha256 = \"{}\"\n",
            sha256_hex(file.content.as_bytes())
        ));
        out.push_str(&format!("executable = {}\n\n", file.executable));
    }
    out
}

pub(crate) fn ensure_trailing_newline(input: &str) -> String {
    if input.ends_with('\n') {
        input.to_string()
    } else {
        format!("{input}\n")
    }
}
