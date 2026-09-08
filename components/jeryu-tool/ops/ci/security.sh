#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cd "$REPO_ROOT"

for path in target target/security target/jankurai target/jankurai/security; do
  if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
    printf 'security output is not a physical directory: %s\n' "$path" >&2
    exit 1
  fi
  [[ -d "$path" ]] || mkdir -m 0750 -- "$path"
  [[ "$(realpath -e -- "$path")" == "$REPO_ROOT/$path" ]]
done

prepare_output() {
  local path="$1"
  if [[ -L "$path" || ( -e "$path" && ! -f "$path" ) ]]; then
    printf 'security output is not a regular file: %s\n' "$path" >&2
    exit 1
  fi
  if [[ -e "$path" ]]; then
    [[ "$(stat -Lc '%u:%h' -- "$path")" == "$EUID:1" ]] || {
      printf 'security output has unsafe custody: %s\n' "$path" >&2
      exit 1
    }
  fi
  rm -f -- "$path"
}

fail=0
findings=()
if [[ -n "$(find . -path './.git' -prune -o -name '.env' -type f -print -quit)" ]]; then
  findings+=("committed .env file")
  fail=1
fi

cargo metadata --locked --offline --format-version 1 --no-deps >/dev/null

require_tool gitleaks
prepare_output target/security/gitleaks.log
gitleaks_status=0
gitleaks detect --source . --redact --no-banner \
  >target/security/gitleaks.log 2>&1 || gitleaks_status=$?
if ((gitleaks_status != 0)); then
  findings+=("gitleaks failed")
  fail=1
fi
printf 'jankurai-security-step={"label":"gitleaks","tool":"gitleaks","shell_command":"gitleaks detect --source . --redact --no-banner","status":"%s","advisory":false,"exit_code":%s}\n' \
  "$([[ $gitleaks_status == 0 ]] && printf ran || printf failed)" "$gitleaks_status"

require_tool actionlint
prepare_output target/security/actionlint.log
actionlint_status=0
actionlint >target/security/actionlint.log 2>&1 || actionlint_status=$?
if ((actionlint_status != 0)); then
  findings+=("actionlint failed")
  fail=1
fi
printf 'jankurai-security-step={"label":"actionlint","tool":"actionlint","shell_command":"actionlint","status":"%s","advisory":false,"exit_code":%s}\n' \
  "$([[ $actionlint_status == 0 ]] && printf ran || printf failed)" "$actionlint_status"

# Enforce dependency/license policy whenever this repository declares one.
if [[ -f deny.toml ]]; then
  require_tool cargo-deny
  prepare_output target/security/cargo-deny.log
  cargo_deny_status=0
  cargo deny check >target/security/cargo-deny.log 2>&1 || cargo_deny_status=$?
  if ((cargo_deny_status != 0)); then
    findings+=("cargo deny failed")
    fail=1
  fi
fi

# Advisory databases are network-backed and run only when explicitly enabled.
if [[ "${JERYU_SECURITY_NETWORK:-0}" == 1 ]]; then
  require_tool cargo-audit
  prepare_output target/security/cargo-audit.json
  prepare_output target/security/cargo-audit.log
  cargo_audit_status=0
  cargo audit --json >target/security/cargo-audit.json \
    2>target/security/cargo-audit.log || cargo_audit_status=$?
  if ((cargo_audit_status != 0)); then
    findings+=("cargo audit failed")
    fail=1
  fi
fi

require_tool syft
syft_version="$(syft version | awk '$1 == "Version:" {print $2}')"
[[ "$syft_version" == 1.40.0 ]] || {
  printf 'expected syft 1.40.0, got %s\n' "${syft_version:-unknown}" >&2
  exit 1
}
prepare_output target/security/sbom.spdx.json
syft dir:. --exclude './target/**' \
  -o spdx-json=target/security/sbom.spdx.json >/dev/null

status=pass
((fail == 0)) || status=fail
findings_json="$(printf '%s\n' "${findings[@]:-}" | jq -Rsc 'split("\n") | map(select(length > 0))')"
prepare_output target/security/evidence.json
jq -cnS --arg status "$status" --argjson findings "$findings_json" '
  {schema_version:"jeryu.tool.security/v1",status:$status,
   checks:["env-file","cargo-metadata","gitleaks","actionlint",
     "cargo-deny-if-configured","cargo-audit-if-network-enabled","syft-sbom"],
   artifacts:{sbom:"target/security/sbom.spdx.json"},findings:$findings}
' >target/security/evidence.json
prepare_output target/jankurai/security/evidence.json
install -m 0644 -- target/security/evidence.json target/jankurai/security/evidence.json
jq -e '.schema_version == "jeryu.tool.security/v1" and
  .status == "pass" and .findings == []' target/security/evidence.json >/dev/null || fail=1
cmp target/security/evidence.json target/jankurai/security/evidence.json
if ((fail != 0)); then
  printf 'security failed; inspect target/security/evidence.json\n' >&2
  exit 1
fi
printf 'security ok: gitleaks actionlint metadata sbom\n'
