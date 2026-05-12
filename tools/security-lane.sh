#!/usr/bin/env bash
set -euo pipefail

repo_root_input="${1:-.}"
if ! repo_root="$(cd "$repo_root_input" && pwd)"; then
  echo "unable to resolve repo root: $repo_root_input" >&2
  exit 1
fi
out_dir="$repo_root/target/jankurai/security"
mkdir -p "$out_dir"

docker_repo_path() {
  local path="$1"
  local rel="${path#"$repo_root"/}"
  printf '/repo/%s' "$rel"
}

json_string() {
  local value="${1//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '"%s"' "$value"
}

gitleaks_report="$out_dir/gitleaks.sarif"
gitleaks_log="$out_dir/gitleaks.log"
cargo_deny_log="$out_dir/cargo-deny.log"
sbom_report="$out_dir/sbom.spdx.json"
syft_log="$out_dir/syft.log"
workflow_lint_log="$out_dir/actionlint.log"
actionlint_bin="$(command -v actionlint || true)"
if [ -z "$actionlint_bin" ] && [ -x "$HOME/go/bin/actionlint" ]; then
  actionlint_bin="$HOME/go/bin/actionlint"
fi

gitleaks_status=0
cargo_deny_status=0
sbom_status=0
workflow_lint_status=0

if command -v gitleaks >/dev/null 2>&1; then
  if gitleaks detect \
    --no-banner \
    --no-git \
    --redact \
    --config "$repo_root/.gitleaks.toml" \
    --source "$repo_root" \
    --report-format sarif \
    --report-path "$gitleaks_report" \
    >"$gitleaks_log" 2>&1; then
    gitleaks_status=0
  else
    gitleaks_status=$?
  fi
elif command -v docker >/dev/null 2>&1; then
  gitleaks_report_docker="$(docker_repo_path "$gitleaks_report")"
  if docker run --rm -v "$repo_root:/repo" -w /repo ghcr.io/gitleaks/gitleaks:v8.30.0 git --no-banner --redact --report-format sarif --report-path "$gitleaks_report_docker" . >"$gitleaks_log" 2>&1; then
    gitleaks_status=0
  else
    gitleaks_status=$?
  fi
else
  echo "security lane needs gitleaks or docker for secret scanning" >&2
  gitleaks_status=127
fi

if cargo deny check >"$cargo_deny_log" 2>&1; then
  cargo_deny_status=0
else
  cargo_deny_status=$?
fi

if command -v syft >/dev/null 2>&1; then
  if syft dir:. -o spdx-json="$sbom_report" >"$syft_log" 2>&1; then
    sbom_status=0
  else
    sbom_status=$?
  fi
elif command -v docker >/dev/null 2>&1; then
  sbom_report_docker="$(docker_repo_path "$sbom_report")"
  if docker run --rm -v "$repo_root:/repo" -w /repo ghcr.io/anchore/syft:v1.40.0 dir:. -o spdx-json="$sbom_report_docker" >"$syft_log" 2>&1; then
    sbom_status=0
  else
    sbom_status=$?
  fi
else
  echo "security lane needs syft or docker for SBOM evidence" >&2
  sbom_status=127
fi

if [ -n "$actionlint_bin" ]; then
  if "$actionlint_bin" "$repo_root/.github/workflows"/*.yml >"$workflow_lint_log" 2>&1; then
    workflow_lint_status=0
  else
    workflow_lint_status=$?
  fi
else
  echo "security lane needs actionlint for workflow scanning" >&2
  workflow_lint_status=127
fi

status=0
if [ "$gitleaks_status" -ne 0 ] || [ "$cargo_deny_status" -ne 0 ] || [ "$sbom_status" -ne 0 ] || [ "$workflow_lint_status" -ne 0 ]; then
  status=1
fi

commit="$(git -C "$repo_root" rev-parse HEAD)"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cat >"$out_dir/evidence.json" <<EOF
{
  "repo_root": $(json_string "$repo_root"),
  "commit": $(json_string "$commit"),
  "generated_at": $(json_string "$generated_at"),
  "secret_scan": {
    "tool": "gitleaks",
    "status": $gitleaks_status,
    "report": "gitleaks.sarif",
    "log": "gitleaks.log"
  },
  "dependency_review": {
    "tool": "cargo deny",
    "status": $cargo_deny_status,
    "report": "cargo-deny.log"
  },
  "sbom": {
    "tool": "syft",
    "status": $sbom_status,
    "report": "sbom.spdx.json",
    "log": "syft.log"
  },
  "workflow_lint": {
    "tool": "actionlint",
    "status": $workflow_lint_status,
    "report": "actionlint.log"
  }
}
EOF

if [ "$status" -ne 0 ]; then
  echo "security lane failed; see $out_dir" >&2
  exit "$status"
fi

echo "security lane evidence written to $out_dir/evidence.json"
