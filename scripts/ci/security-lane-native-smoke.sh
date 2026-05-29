#!/usr/bin/env bash
# Verify the security lane runs with native tool stubs and never falls back
# to Docker for secret scanning or SBOM generation.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

tmp_root="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT

stub_dir="$tmp_root/bin"
mkdir -p "$stub_dir"

cat >"$stub_dir/gitleaks" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

report_path=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --report-path)
      shift
      report_path="${1:-}"
      ;;
  esac
  shift || true
done

if [ -z "$report_path" ]; then
  echo "gitleaks stub missing --report-path" >&2
  exit 1
fi

mkdir -p "$(dirname "$report_path")"
cat >"$report_path" <<'JSON'
{"version":"2.1.0","runs":[]}
JSON
printf 'gitleaks stub wrote %s\n' "$report_path"
EOF
chmod +x "$stub_dir/gitleaks"

cat >"$stub_dir/cargo-deny" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'cargo-deny stub %s\n' "$*"
EOF
chmod +x "$stub_dir/cargo-deny"

cat >"$stub_dir/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = "deny" ] && [ "${2:-}" = "check" ]; then
  printf 'cargo deny check stub\n'
  exit 0
fi

echo "unexpected cargo invocation: $*" >&2
exit 1
EOF
chmod +x "$stub_dir/cargo"

cat >"$stub_dir/syft" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

report_path=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      shift
      case "${1:-}" in
        spdx-json=*)
          report_path="${1#spdx-json=}"
          ;;
      esac
      ;;
    -o=spdx-json=*)
      report_path="${1#-o=spdx-json=}"
      ;;
    -o*)
      case "${1#-o}" in
        spdx-json=*)
          report_path="${1#-ospdx-json=}"
          ;;
      esac
      ;;
  esac
  shift || true
done

if [ -z "$report_path" ]; then
  echo "syft stub missing spdx-json output path" >&2
  exit 1
fi

mkdir -p "$(dirname "$report_path")"
cat >"$report_path" <<'JSON'
{"spdxVersion":"SPDX-2.3","packages":[]}
JSON
printf 'syft stub wrote %s\n' "$report_path"
EOF
chmod +x "$stub_dir/syft"

cat >"$stub_dir/actionlint" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'actionlint stub %s\n' "$*"
EOF
chmod +x "$stub_dir/actionlint"

cat >"$stub_dir/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ -n "${DOCKER_SENTINEL:-}" ]; then
  mkdir -p "$(dirname "$DOCKER_SENTINEL")"
  touch "$DOCKER_SENTINEL"
fi
echo "docker must not be invoked by the security lane" >&2
exit 99
EOF
chmod +x "$stub_dir/docker"

DOCKER_SENTINEL="$tmp_root/docker-invoked" \
PATH="$stub_dir:$PATH" \
  bash "$REPO_ROOT/tools/security-lane.sh" "$REPO_ROOT"

if [ -e "$tmp_root/docker-invoked" ]; then
  echo "security lane invoked docker unexpectedly" >&2
  exit 1
fi

for artifact in \
  "$REPO_ROOT/target/jankurai/security/evidence.json" \
  "$REPO_ROOT/target/jankurai/security/gitleaks.sarif" \
  "$REPO_ROOT/target/jankurai/security/cargo-deny.log" \
  "$REPO_ROOT/target/jankurai/security/sbom.spdx.json" \
  "$REPO_ROOT/target/jankurai/security/actionlint.log"
do
  if [ ! -f "$artifact" ]; then
    echo "missing security-lane artifact: $artifact" >&2
    exit 1
  fi
done

printf 'security lane native smoke passed\n'
