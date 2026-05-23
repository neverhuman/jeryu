#!/usr/bin/env bash
set -euo pipefail

fleet_root="${VEOX_FLEET_ROOT:-/home/ubuntu/veox-repos}"
jeryu_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
report_dir="${JERYU_FUSION_REPORT_DIR:-${jeryu_root}/target/veox-fusion}"
report="${report_dir}/report.json"
passes_file="${report_dir}/passes.txt"
failures_file="${report_dir}/failures.txt"

shared_tag="${VEOX_SHARED_TAG:-veox-shared-v0.1.1-split.1}"
proofs_tag="${VEOX_PROOFS_TAG:-veox-proofs-v0.1.1-split.2}"
deploy_tag="${VEOX_DEPLOY_TAG:-veox-deploy-v0.1.1-split.2}"
nht_tag="${VEOX_NHT_TAG:-veox-nht-v0.1.1-split.2}"

repos=(
  "shared:veox-shared:rust-workspace"
  "proofs:veox-proofs:rust-workspace"
  "data:veox-neverhuman-data:data-client"
  "docs:veox-docs-meta:docs-meta"
  "enclave:veox-enclave:rust-workspace"
  "nht:veox-nht:rust-workspace"
  "warp:veox-warp:node-frontend"
  "catalog:veox-stage-catalog:artifact-catalog"
  "deploy:veox-deploy:rust-workspace"
)

mkdir -p "${report_dir}"
: >"${passes_file}"
: >"${failures_file}"

note_pass() {
  printf '%s\n' "$1" >>"${passes_file}"
}

note_fail() {
  printf '%s\n' "$1" >>"${failures_file}"
}

require_file() {
  local label="$1"
  local path="$2"
  if [[ -f "${path}" ]]; then
    note_pass "${label}: ${path}"
  else
    note_fail "${label}: missing ${path}"
  fi
}

require_tag() {
  local repo="$1"
  local tag="$2"
  local root="${fleet_root}/${repo}"
  if git -C "${root}" rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    note_pass "${repo}: tag ${tag} exists locally"
  else
    note_fail "${repo}: missing local tag ${tag}"
  fi
}

require_rg() {
  local label="$1"
  local needle="$2"
  local root="$3"
  if rg -n "${needle}" "${root}" --glob 'Cargo.toml' --glob '!**/target/**' >/dev/null; then
    note_pass "${label}"
  else
    note_fail "${label}: no Cargo.toml match for ${needle}"
  fi
}

audit_path_dependencies() {
  local repo="$1"
  local root="$2"
  python3 - "${repo}" "${root}" <<'PY'
import os
import pathlib
import sys
import tomllib

repo = sys.argv[1]
root = pathlib.Path(sys.argv[2]).resolve()
ok = True

def dep_sections(data):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        section = data.get(key)
        if isinstance(section, dict):
            yield key, section
    target = data.get("target")
    if isinstance(target, dict):
        for target_name, target_data in target.items():
            if not isinstance(target_data, dict):
                continue
            for key in ("dependencies", "dev-dependencies", "build-dependencies"):
                section = target_data.get(key)
                if isinstance(section, dict):
                    yield f"target.{target_name}.{key}", section

for manifest in root.rglob("Cargo.toml"):
    parts = set(manifest.parts)
    if ".git" in parts or "target" in parts:
        continue
    data = tomllib.loads(manifest.read_text())
    for section_name, section in dep_sections(data):
        for dep_name, spec in section.items():
            if not isinstance(spec, dict) or "path" not in spec:
                continue
            resolved = (manifest.parent / spec["path"]).resolve()
            try:
                resolved.relative_to(root)
            except ValueError:
                print(
                    f"{repo}: {manifest.relative_to(root)} {section_name}.{dep_name} points outside repo: {spec['path']}",
                    file=sys.stderr,
                )
                ok = False

sys.exit(0 if ok else 1)
PY
}

blocked_source_pattern='/home/ubuntu/veox(/|$)|/home/ubuntu/veox-split|/home/ubuntu/jansu|\.\./jansu'

for spec in "${repos[@]}"; do
  IFS=: read -r alias repo profile <<<"${spec}"
  root="${fleet_root}/${repo}"
  expected_origin="https://github.com/veox-systems/${repo}.git"
  expected_source="https://github.com/neverhuman/${repo}.git"

  if [[ -d "${root}/.git" ]]; then
    note_pass "${repo}: fresh clone exists at ${root}"
  else
    note_fail "${repo}: missing git clone at ${root}"
    continue
  fi

  origin="$(git -C "${root}" remote get-url origin 2>/dev/null || true)"
  if [[ "${origin}" == "${expected_origin}" ]]; then
    note_pass "${repo}: origin remote targets veox-systems"
  else
    note_fail "${repo}: origin remote is ${origin:-<missing>}, expected ${expected_origin}"
  fi

  source="$(git -C "${root}" remote get-url source 2>/dev/null || true)"
  if [[ -z "${source}" || "${source}" == "${expected_source}" ]]; then
    note_pass "${repo}: source fallback remote is valid"
  else
    note_fail "${repo}: source fallback remote is ${source}, expected ${expected_source}"
  fi

  if git -C "${root}" status --porcelain --untracked-files=no | grep -q .; then
    note_fail "${repo}: tracked worktree is dirty"
  else
    note_pass "${repo}: tracked worktree is clean"
  fi

  for recipe in fast score check; do
    if (cd "${root}" && just --list | sed -E 's/^[[:space:]]+//' | cut -d' ' -f1 | grep -qx "${recipe}"); then
      note_pass "${repo}: just ${recipe} exists"
    else
      note_fail "${repo}: missing just ${recipe}"
    fi
  done

  require_file "${repo}: jankurai score json" "${root}/target/jankurai/repo-score.json"
  require_file "${repo}: jankurai score markdown" "${root}/target/jankurai/repo-score.md"

  if hits="$(rg -n "${blocked_source_pattern}" "${root}" \
      --glob '!**/.git/**' \
      --glob '!**/target/**' \
      --glob '!**/node_modules/**' \
      --glob 'Cargo.toml' \
      --glob 'package.json' \
      --glob 'package-lock.json' \
      --glob 'pnpm-lock.yaml' \
      --glob 'justfile' \
      --glob '*.sh' \
      --glob '.github/workflows/*.yml' \
      --glob '.github/workflows/*.yaml' 2>/dev/null)"; then
    note_fail "${repo}: source/staging path references remain: ${hits//$'\n'/; }"
  else
    status=$?
    if [[ "${status}" -eq 1 ]]; then
      note_pass "${repo}: manifest/just/CI source path audit clean"
    else
      note_fail "${repo}: source path audit command failed with status ${status}"
    fi
  fi

  if dep_output="$(audit_path_dependencies "${repo}" "${root}" 2>&1)"; then
    note_pass "${repo}: Cargo path dependencies stay inside repo"
  else
    note_fail "${repo}: Cargo path dependency audit failed: ${dep_output//$'\n'/; }"
  fi

  if [[ "${profile}" == "node-frontend" ]]; then
    if find "${root}" -type f \( -name 'package-lock.json' -o -name 'pnpm-lock.yaml' -o -name 'yarn.lock' -o -name 'bun.lockb' -o -name 'npm-shrinkwrap.json' \) \
      -not -path '*/node_modules/*' \
      -not -path '*/target/*' \
      -print -quit | grep -q .; then
      note_pass "${repo}: repo-local JS lockfile exists"
    else
      note_fail "${repo}: missing repo-local JS lockfile"
    fi
  fi
done

require_tag "veox-shared" "${shared_tag}"
require_tag "veox-proofs" "${proofs_tag}"
require_tag "veox-deploy" "${deploy_tag}"
require_tag "veox-nht" "${nht_tag}"

require_rg "veox-shared consumes immutable Jansu tag" "jansu-v0.6.3-split.1" "${fleet_root}/veox-shared"
require_rg "veox-deploy consumes shared split tag" "${shared_tag}" "${fleet_root}/veox-deploy"
require_rg "veox-deploy consumes proofs split tag" "${proofs_tag}" "${fleet_root}/veox-deploy"
require_rg "veox-nht consumes shared split tag" "${shared_tag}" "${fleet_root}/veox-nht"
require_rg "veox-nht consumes deploy split tag" "${deploy_tag}" "${fleet_root}/veox-nht"
require_rg "veox-enclave consumes shared split tag" "${shared_tag}" "${fleet_root}/veox-enclave"
require_rg "veox-enclave consumes nht split tag" "${nht_tag}" "${fleet_root}/veox-enclave"

if unpinned="$(rg -n 'git = "https://github.com/neverhuman/veox-' "${fleet_root}" \
    --glob 'Cargo.toml' \
    --glob '!**/target/**' 2>/dev/null | grep -v 'tag = ' | grep -v 'rev = ' || true)" && [[ -n "${unpinned}" ]]; then
  note_fail "private veox git dependencies without tag/rev: ${unpinned//$'\n'/; }"
else
  note_pass "all private veox git dependencies use tag or rev pins"
fi

require_file "shared canonical contract schema" "${fleet_root}/veox-shared/contracts/events/queue-event.v1.schema.json"
require_file "shared generated contract manifest" "${fleet_root}/veox-shared/contracts/generated/events/queue-event.v1.manifest.md"
require_file "shared Rust contract crate" "${fleet_root}/veox-shared/crates/veox-contracts/Cargo.toml"
require_file "catalog signed seed manifest" "${fleet_root}/veox-stage-catalog/seed_data/.veox-stage-manifest.json"
require_file "data catalog manifest" "${fleet_root}/veox-neverhuman-data/NeverHumanData/catalog-manifest.json"
require_file "data catalog signature" "${fleet_root}/veox-neverhuman-data/NeverHumanData/catalog-manifest.sig"
require_file "deploy lockfile" "${fleet_root}/veox-deploy/Cargo.lock"

python3 - "${report}" "${passes_file}" "${failures_file}" "${shared_tag}" "${proofs_tag}" "${deploy_tag}" "${nht_tag}" <<'PY'
import datetime
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
passes = pathlib.Path(sys.argv[2]).read_text().splitlines()
failures = pathlib.Path(sys.argv[3]).read_text().splitlines()
shared_tag, proofs_tag, deploy_tag, nht_tag = sys.argv[4:8]

payload = {
    "schema_version": "1",
    "name": "veox-fusion",
    "kind": "mocked-contract-stack",
    "generated_at": datetime.datetime.now(datetime.UTC).isoformat(),
    "status": "pass" if not failures else "fail",
    "target_namespace": "veox-systems",
    "source_namespace_fallback": "neverhuman",
    "inputs": {
        "shared_contracts_tag": shared_tag,
        "proofs_tag": proofs_tag,
        "deploy_tag": deploy_tag,
        "nht_tag": nht_tag,
    },
    "mocks": {
        "enclave_service": "fake-enclave-gateway",
        "nht_service": "fake-nht-gateway",
        "image_digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        "seed_bundle": "veox-stage-catalog:seed_data/.veox-stage-manifest.json",
        "data_catalog": "veox-neverhuman-data:NeverHumanData/catalog-manifest.json",
        "release_manifest": "fake-release-manifest:v0",
    },
    "passes": passes,
    "failures": failures,
}
report_path.write_text(json.dumps(payload, indent=2) + "\n")
print(f"veox-fusion {payload['status']}: {len(passes)} passes, {len(failures)} failures")
print(f"report: {report_path}")
PY

if [[ -s "${failures_file}" ]]; then
  exit 1
fi
