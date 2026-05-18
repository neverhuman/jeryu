#!/usr/bin/env bash
set -euo pipefail

# Semantic Versioning Enforcement Script
# Validates version consistency across all sources and adherence to semver.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

VERSION_FILE="$REPO_ROOT/VERSION"
VERSION_JSON="$REPO_ROOT/version.json"
CARGO_TOML="$REPO_ROOT/Cargo.toml"
CHANGELOG="$REPO_ROOT/CHANGELOG.md"

# Check dependencies
for cmd in jq grep sed; do
  if ! command -v "$cmd" &> /dev/null; then
    echo "Error: $cmd is required but not installed"
    exit 1
  fi
done

# Read versions
if [ ! -f "$VERSION_FILE" ]; then
  echo "Error: VERSION file not found at $VERSION_FILE"
  exit 1
fi
VERSION_FROM_FILE=$(tr -d '[:space:]' < "$VERSION_FILE")

if [ ! -f "$VERSION_JSON" ]; then
  echo "Error: version.json not found at $VERSION_JSON"
  exit 1
fi
VERSION_FROM_JSON=$(jq -r '.version' "$VERSION_JSON")
STATUS_FROM_JSON=$(jq -r '.status' "$VERSION_JSON")

if [ ! -f "$CARGO_TOML" ]; then
  echo "Error: Cargo.toml not found at $CARGO_TOML"
  exit 1
fi
# Extract workspace.package version from Cargo.toml
VERSION_FROM_CARGO=$(awk -F'"' '
  /^\[workspace\.package\]$/ { in_workspace=1; next }
  /^\[/ && in_workspace { exit }
  in_workspace && /^version = "/ { print $2; exit }
' "$CARGO_TOML")

# Validate semver format (x.y.z or x.y.z-prerelease)
SEMVER_REGEX='^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'
if ! echo "$VERSION_FROM_FILE" | grep -qE "$SEMVER_REGEX"; then
  echo "Error: VERSION file version '$VERSION_FROM_FILE' does not follow semver (x.y.z or x.y.z-prerelease)"
  exit 1
fi

# Check version consistency across all sources
if [ "$VERSION_FROM_FILE" != "$VERSION_FROM_JSON" ]; then
  echo "Error: Version mismatch between VERSION file ($VERSION_FROM_FILE) and version.json ($VERSION_FROM_JSON)"
  exit 1
fi

if [ "$VERSION_FROM_FILE" != "$VERSION_FROM_CARGO" ]; then
  echo "Error: Version mismatch between VERSION file ($VERSION_FROM_FILE) and Cargo.toml workspace version ($VERSION_FROM_CARGO)"
  exit 1
fi

# Validate status matches version type
if echo "$VERSION_FROM_FILE" | grep -qE '-'; then
  # Prerelease version
  if [ "$STATUS_FROM_JSON" != "rc" ] && [ "$STATUS_FROM_JSON" != "development" ]; then
    echo "Error: Prerelease version $VERSION_FROM_FILE should have status 'rc' or 'development' in version.json, got '$STATUS_FROM_JSON'"
    exit 1
  fi
else
  # Stable version
  if [ "$STATUS_FROM_JSON" != "stable" ]; then
    echo "Error: Stable version $VERSION_FROM_FILE should have status 'stable' in version.json, got '$STATUS_FROM_JSON'"
    exit 1
  fi
fi

# Check CHANGELOG entry
if [ ! -f "$CHANGELOG" ]; then
  echo "Error: CHANGELOG.md not found at $CHANGELOG"
  exit 1
fi

if ! grep -qF "## [$VERSION_FROM_FILE]" "$CHANGELOG"; then
  echo "Error: CHANGELOG.md missing entry for version $VERSION_FROM_FILE"
  echo "Expected format: ## [$VERSION_FROM_FILE] - YYYY-MM-DD"
  exit 1
fi

# Validate tag_policy in version.json matches version
TAG_POLICY_RC=$(jq -r '.tag_policy.rc' "$VERSION_JSON" | sed 's/v//' | sed 's/\.N$//')
TAG_POLICY_STABLE=$(jq -r '.tag_policy.stable' "$VERSION_JSON" | sed 's/v//')

if echo "$VERSION_FROM_FILE" | grep -qE '-'; then
  # Prerelease: check RC policy matches base version
  BASE_VERSION=$(echo "$VERSION_FROM_FILE" | sed 's/-.*//')
  if [ "$TAG_POLICY_RC" != "$BASE_VERSION" ] && [ "$TAG_POLICY_RC" != "$VERSION_FROM_FILE" ]; then
    echo "Warning: tag_policy.rc ($TAG_POLICY_RC) does not match version $VERSION_FROM_FILE"
  fi
else
  # Stable: check stable policy matches version
  if [ "$TAG_POLICY_STABLE" != "$VERSION_FROM_FILE" ]; then
    echo "Error: tag_policy.stable ($TAG_POLICY_STABLE) does not match version $VERSION_FROM_FILE"
    exit 1
  fi
fi

echo "✅ Semantic versioning enforcement passed: version $VERSION_FROM_FILE is valid and consistent across all sources."
exit 0
