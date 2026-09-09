#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh

mkdir -p target/jankurai/security target/security
checks_tsv="target/jankurai/security/checks.tsv"
evidence_json="target/jankurai/security/evidence.json"
: > "$checks_tsv"
failed=0
require_security_tools="${JERYU_REQUIRE_SECURITY_TOOLS:-1}"
[[ "${require_security_tools}" == "0" || "${require_security_tools}" == "1" ]] || {
  printf 'JERYU_REQUIRE_SECURITY_TOOLS must be 0 or 1\n' >&2
  exit 1
}

record() {
  local name="$1"
  local status="$2"
  local policy="$3"
  local detail="$4"
  printf '%s\t%s\t%s\t%s\n' "$name" "$status" "$policy" "$detail" >> "$checks_tsv"
}

mark_missing_tool() {
  local name="$1"
  local tool="$2"
  if [[ "$require_security_tools" == "1" ]]; then
    record "$name" "fail" "required" "$tool is not installed"
    failed=1
  else
    record "$name" "not_run" "tool-missing" "$tool is not installed; set JERYU_REQUIRE_SECURITY_TOOLS=1 to require it"
  fi
}

write_evidence() {
  python3 - "$checks_tsv" "$evidence_json" <<'PY'
import json
import sys
from pathlib import Path

checks = []
for line in Path(sys.argv[1]).read_text().splitlines():
    name, status, policy, detail = line.split("\t", 3)
    checks.append({
        "name": name,
        "status": status,
        "policy": policy,
        "detail": detail,
    })

Path(sys.argv[2]).write_text(json.dumps({
    "schema_version": "jeryu.split.security/v1",
    "checks": checks,
}, indent=2, sort_keys=True) + "\n")
PY
}

if find . -path './.git' -prune -o -path './target' -prune -o -name '.env' -type f -print | grep -q .; then
  record "env-file" "fail" "required" "repository contains a .env file"
  failed=1
else
  record "env-file" "pass" "required" "no .env files found outside ignored build output"
fi

if cargo metadata --format-version 1 --no-deps >/dev/null; then
  record "cargo-metadata" "pass" "required" "workspace dependency metadata resolves"
else
  record "cargo-metadata" "fail" "required" "cargo metadata failed"
  failed=1
fi

if [[ -s Cargo.lock ]]; then
  sha256sum Cargo.lock > target/jankurai/security/Cargo.lock.sha256
  cp target/jankurai/security/Cargo.lock.sha256 target/security/Cargo.lock.sha256
  record "sbom-provenance" "pass" "lock-digest" "SBOM provenance input digest recorded in target/jankurai/security/Cargo.lock.sha256"
else
  record "sbom-provenance" "fail" "lock-digest" "Cargo.lock is missing"
  failed=1
fi

if command -v syft >/dev/null 2>&1; then
  syft_version="$(syft version -o json | jq -r '.version // empty')"
  if [[ "${syft_version}" != "1.40.0" ]]; then
    record "cyclonedx-sbom" "fail" "syft-1.40.0" \
      "unexpected syft version: ${syft_version:-missing}"
    failed=1
  elif syft scan dir:. --source-name jeryu-jira \
      --source-version "$(<VERSION)" --exclude './target/**' \
      --exclude './.git/**' \
      --output cyclonedx-json=target/security/sbom.cdx.json >/dev/null; then
    if jq -e --arg version "$(<VERSION)" \
      '.bomFormat == "CycloneDX" and
       .metadata.component.name == "jeryu-jira" and
       .metadata.component.version == $version and
       (.components | type == "array")' \
      target/security/sbom.cdx.json >/dev/null; then
      record "cyclonedx-sbom" "pass" "syft-1.40.0" \
        "CycloneDX source inventory emitted at target/security/sbom.cdx.json"
    else
      record "cyclonedx-sbom" "fail" "syft-1.40.0" \
        "CycloneDX metadata does not bind the Jira source identity"
      failed=1
    fi
  else
    record "cyclonedx-sbom" "fail" "syft-1.40.0" "syft scan failed"
    failed=1
  fi
else
  mark_missing_tool "cyclonedx-sbom" "syft"
fi

if command -v cargo-audit >/dev/null 2>&1; then
  if cargo audit --deny warnings; then
    record "dependency-audit" "pass" "cargo-audit" "cargo audit completed"
  else
    record "dependency-audit" "fail" "cargo-audit" "cargo audit reported advisories"
    failed=1
  fi
else
  mark_missing_tool "dependency-audit" "cargo-audit"
fi

if command -v gitleaks >/dev/null 2>&1; then
  set +e
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    {
      git ls-files -z
      git ls-files --others --exclude-standard -z
    } | sort -zu | while IFS= read -r -d '' path; do
      [[ -f "$path" ]] || continue
      case "$path" in
        target/*)
          continue
          ;;
      esac
      if LC_ALL=C grep -Iq . "$path"; then
        printf '\n===== %s =====\n' "$path"
        cat "$path"
      fi
    done | gitleaks detect --pipe --redact --verbose >/tmp/jeryu-jira-gitleaks.log 2>&1
    secret_status="$?"
  else
    gitleaks detect --no-git --redact --verbose >/tmp/jeryu-jira-gitleaks.log 2>&1
    secret_status="$?"
  fi
  set -e
  if [[ "$secret_status" == "0" ]]; then
    record "secret-scan" "pass" "gitleaks" "gitleaks completed"
  else
    record "secret-scan" "fail" "gitleaks" "gitleaks reported findings; see /tmp/jeryu-jira-gitleaks.log"
    failed=1
  fi
else
  mark_missing_tool "secret-scan" "gitleaks"
fi

if [[ -d .github/workflows ]]; then
  if command -v actionlint >/dev/null 2>&1; then
    if actionlint .github/workflows/*.yml; then
      record "actionlint" "pass" "actionlint" "workflow lint completed"
    else
      record "actionlint" "fail" "actionlint" "workflow lint reported errors"
      failed=1
    fi
  else
    mark_missing_tool "actionlint" "actionlint"
  fi
else
  record "actionlint" "not_run" "no-workflows" "no GitHub workflow files are present"
fi

write_evidence
cp "$evidence_json" target/security/evidence.json

if [[ "$failed" != "0" ]]; then
  printf 'security check failed; evidence written to %s\n' "$evidence_json" >&2
  exit 1
fi

printf 'security ok; evidence written to %s\n' "$evidence_json"
