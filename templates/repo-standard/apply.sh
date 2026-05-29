#!/usr/bin/env bash
# Manual apply helper for the jeryu repo-standard template tree.
#
# This is a SHIM. The eventual Rust implementation lives in `src/repo_standard/`
# behind `jeryu repo quickstart && jeryu repo standard apply`. Until that CLI
# wiring lands (blocked on coordination of src/lib.rs/cli_defs.rs/dispatch.rs),
# operators and agents can run this script to apply the templates directly:
#
#   bash templates/repo-standard/apply.sh \
#     --target /path/to/repo \
#     --repo-namespace acme --repo-name widget --default-branch main \
#     [--offline-mirror-enabled true --external-remote ssh://github/acme/widget.git]
#
# The script writes the same files the Rust apply would write and emits a
# minimal .jeryu/standard.lock with sha256s of the rendered output, so the
# eventual `jeryu repo standard verify` can detect drift.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JERYU_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

target=""
ns=""
name=""
branch=""
mirror_enabled="false"
external_remote=""

usage() {
  cat <<EOF
Usage: $0 --target <repo-dir> --repo-namespace <ns> --repo-name <name>
         --default-branch <branch> [--offline-mirror-enabled true|false]
         [--external-remote <url>]
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    --repo-namespace) ns="$2"; shift 2 ;;
    --repo-name) name="$2"; shift 2 ;;
    --default-branch) branch="$2"; shift 2 ;;
    --offline-mirror-enabled) mirror_enabled="$2"; shift 2 ;;
    --external-remote) external_remote="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

if [ -z "$target" ] || [ -z "$ns" ] || [ -z "$name" ] || [ -z "$branch" ]; then
  usage; exit 2
fi
if [ ! -d "$target" ]; then
  echo "target dir does not exist: $target" >&2; exit 2
fi

log() { printf '\033[1;34m[apply]\033[0m %s\n' "$*" >&2; }

# Substitute {{TOKEN}} placeholders into a file copy.
subst() {
  local src="$1" dst="$2"
  sed \
    -e "s|{{REPO_NAMESPACE}}|${ns}|g" \
    -e "s|{{REPO_NAME}}|${name}|g" \
    -e "s|{{DEFAULT_BRANCH}}|${branch}|g" \
    -e "s|{{OFFLINE_MIRROR_ENABLED}}|${mirror_enabled}|g" \
    -e "s|{{EXTERNAL_REMOTE}}|${external_remote}|g" \
    "$src" > "$dst"
}

install_file() {
  local src="$1" rel="$2" mode="$3"
  local dst="$target/$rel"
  mkdir -p "$(dirname "$dst")"
  subst "$src" "$dst"
  chmod "$mode" "$dst"
  log "wrote $rel"
}

copy_canonical() {
  local src="$1" rel="$2" mode="$3"
  local dst="$target/$rel"
  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst"
  chmod "$mode" "$dst"
  log "wrote $rel (canonical from $src)"
}

# ── managed files (order matches manifest.toml) ───────────────────────────
install_file "$SCRIPT_DIR/.gitlab-ci.yml"            ".gitlab-ci.yml"            0644
install_file "$SCRIPT_DIR/.gitlab/ci/_shared.yml"    ".gitlab/ci/_shared.yml"    0644
install_file "$SCRIPT_DIR/.jeryu/repo.toml"          ".jeryu/repo.toml"          0644
install_file "$SCRIPT_DIR/.jeryu/policy.toml"        ".jeryu/policy.toml"        0644
install_file "$SCRIPT_DIR/.jeryu/ci.toml"            ".jeryu/ci.toml"            0644
install_file "$SCRIPT_DIR/.jeryu/backup.toml"        ".jeryu/backup.toml"        0644
copy_canonical "$JERYU_ROOT/scripts/jankurai-manifest.json" ".jeryu/ci/jankurai-manifest.json" 0644
copy_canonical "$JERYU_ROOT/scripts/install-jankurai.sh"    ".jeryu/ci/install-jankurai.sh"    0755
copy_canonical "$JERYU_ROOT/scripts/ci/no-runner-tags.sh"   ".jeryu/ci/no-runner-tags.sh"      0755

# ── lock file ─────────────────────────────────────────────────────────────
lock="$target/.jeryu/standard.lock"
{
  printf 'schema_version = "2"\n'
  printf 'template = "jeryu-repo-standard"\n'
  printf 'template_version = "agent-first-quickstart-v1"\n'
  printf 'applied_at = "%s"\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'applied_by = "templates/repo-standard/apply.sh"\n\n'
  printf '[substitutions]\n'
  printf 'REPO_NAMESPACE = "%s"\n' "$ns"
  printf 'REPO_NAME = "%s"\n' "$name"
  printf 'DEFAULT_BRANCH = "%s"\n' "$branch"
  printf 'OFFLINE_MIRROR_ENABLED = "%s"\n' "$mirror_enabled"
  printf 'EXTERNAL_REMOTE = "%s"\n\n' "$external_remote"
  printf '[[managed_file]]\n'
  for rel in .gitlab-ci.yml .gitlab/ci/_shared.yml .jeryu/repo.toml \
             .jeryu/policy.toml .jeryu/ci.toml .jeryu/backup.toml \
             .jeryu/ci/jankurai-manifest.json .jeryu/ci/install-jankurai.sh \
             .jeryu/ci/no-runner-tags.sh; do
    sha="$(sha256sum "$target/$rel" | awk '{print $1}')"
    printf 'path = "%s"\n' "$rel"
    printf 'sha256 = "%s"\n' "$sha"
    case "$rel" in *.sh) printf 'executable = true\n\n[[managed_file]]\n' ;; *) printf 'executable = false\n\n[[managed_file]]\n' ;; esac
  done
} | sed '$d' | sed '$d' > "$lock"   # strip trailing empty [[managed_file]]

log "wrote .jeryu/standard.lock (schema_version=2)"
log "done — review the rendered files and commit"
