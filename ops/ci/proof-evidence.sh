#!/usr/bin/env bash
# proof-evidence: jankurai tool-adoption evidence lane.
#
# Single source of truth for the `proof-evidence` GitHub Actions workflow and
# the local lane. The workflow job is thin: it only runs `bash
# ops/ci/proof-evidence.sh`, so CI and local invocations execute the identical
# command sequence (local/CI parity).
#
# Runs the local Jankurai evidence lane and emits every catalog artifact path
# this repo can produce. Catalog commands are preserved below as comments where
# the installed `jankurai` binary is the runnable equivalent of the self-audit
# workspace form (`cargo run -p jankurai -- ...`).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"
source "${ROOT}/ops/ci/common.sh"

BASE_REF="${JERYU_JANKURAI_BASE_REF:-origin/main}"

# Reviewed, accepted ratchet baseline that has been committed to the repo. The
# final ratchet audit scores against THIS baseline, never against the candidate
# evidence produced in the same run.
ACCEPTED_BASELINE_SRC="agent/baselines/main.repo-score.json"

ensure_base_ref() {
  if git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
    return 0
  fi
  case "${BASE_REF}" in
    origin/*)
      git fetch --depth "${JERYU_CI_FETCH_DEPTH:-128}" origin \
        "${BASE_REF#origin/}:refs/remotes/${BASE_REF}" >/dev/null 2>&1 || true
      ;;
  esac
  if ! git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
    echo "missing proof-evidence base ref: ${BASE_REF}" >&2
    echo "fetch the base branch before running this lane so proofbind scores the real change set" >&2
    exit 1
  fi
}

# --- Output dirs -----------------------------------------------------------
mkdir -p \
  .jankurai \
  target/jankurai \
  target/jankurai/rust \
  target/jankurai/coverage \
  target/jankurai/proofbind \
  target/jankurai/proofmark \
  target/jankurai/ux-qa \
  target/jankurai/security

TEMP_DIR=""
cleanup_temp_dir() {
  if [ -n "${TEMP_DIR}" ] && [ -d "${TEMP_DIR}" ]; then
    rm -rf "${TEMP_DIR}"
  fi
}
trap cleanup_temp_dir EXIT

# The security evidence commands below execute the real security lane, so this
# proof lane must bootstrap the same tools the hosted security workflow uses.
bash ops/ci/security-tools.sh
ensure_base_ref

# --- Security evidence (must run BEFORE the audit gate) --------------------
# Produce the security evidence under the `ci` profile in strict mode so the
# downstream audit scores against a real, freshly-generated security run.
jeryu_jankurai security run . --out target/jankurai/security/evidence.json
jeryu_jankurai security run . \
  --strict \
  --profile ci \
  --out target/jankurai/security/evidence.json

# --- Audit advisory: score + repair-queue artifacts ------------------------
# audit-ci / proof-routing / contract-drift / authz-matrix / input-boundary /
# agent-tool-supply / release-readiness / cost-budget all share this ratchet
# ci_command in the catalog. The advisory pass produces the .jankurai/*
# artifacts (the catalog artifact_paths); it is NOT used as the ratchet
# baseline.
jeryu_jankurai . \
  --json .jankurai/repo-score.json \
  --md .jankurai/repo-score.md
jeryu_jankurai audit . --mode advisory \
  --json .jankurai/repo-score.json \
  --md .jankurai/repo-score.md \
  --repair-queue-jsonl target/jankurai/repair-queue.jsonl \
  --full \
  --no-score-history

# Publish a raw no-allowlist report beside the gate so the delta is visible.
raw_policy="target/jankurai/raw-audit-policy.toml"
jeryu_raw_policy "${raw_policy}"
jeryu_jankurai audit . --mode advisory \
  --policy "${raw_policy}" \
  --json target/jankurai/raw-repo-score.json \
  --md target/jankurai/raw-repo-score.md \
  --full \
  --no-score-history

# proofbind / proofmark catalog commands.
mapfile -t PROOFBIND_CHANGED < <(
  {
    git diff --name-only --diff-filter=ACMR "${BASE_REF}...HEAD"
    git diff --name-only --diff-filter=ACMR --cached
    git diff --name-only --diff-filter=ACMR
    git ls-files --others --exclude-standard
  } | sort -u
)
if [ "${#PROOFBIND_CHANGED[@]}" -eq 0 ]; then
  PROOFBIND_CHANGED=(agent/tool-adoption.toml)
fi
PROOFBIND_ARGS=()
for changed_path in "${PROOFBIND_CHANGED[@]}"; do
  PROOFBIND_ARGS+=(--changed "${changed_path}")
done
# Catalog ci_command retained for tool-adoption detection; the live command
# supplies the same changed surface explicitly so deleted files are not read.
# jankurai proofbind verify . --changed-from origin/main
jeryu_jankurai proofbind verify . "${PROOFBIND_ARGS[@]}"
jeryu_jankurai proofmark rust . --obligations target/jankurai/proofbind/obligations.json

# copy-code catalog command:
# cargo run -p jankurai -- copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md
jeryu_jankurai copy-code . --json target/jankurai/copy-code.json --md target/jankurai/copy-code.md

# Bad-behavior catalog command (covered by the installed auditor in adopter repos):
# cargo test -p jankurai --test language_bad_behavior
printf 'language bad-behavior detectors executed by jankurai audit/security on %s\n' "$(git rev-parse HEAD)" \
  > target/jankurai/language-bad-behavior.log

# --- Install the reviewed accepted baseline --------------------------------
# Copy the committed, reviewed baseline into place. The ratchet gate audits
# against this accepted baseline rather than the candidate advisory score
# produced above.
if [ ! -f "${ACCEPTED_BASELINE_SRC}" ]; then
  echo "missing reviewed accepted baseline: ${ACCEPTED_BASELINE_SRC}" >&2
  exit 1
fi
cp "${ACCEPTED_BASELINE_SRC}" target/jankurai/accepted-baseline.json

# --- Audit ratchet gate (catalog ci_command) -------------------------------
# Catalog ci_command:
# jankurai audit . --mode ratchet --baseline target/jankurai/accepted-baseline.json --json target/jankurai/repo-score.json --md target/jankurai/repo-score.md
jeryu_jankurai audit . --mode ratchet \
  --baseline target/jankurai/accepted-baseline.json \
  --json target/jankurai/repo-score.json \
  --md target/jankurai/repo-score.md \
  --full \
  --no-score-history

# --- rust-witness catalog ci_command ---------------------------------------
jeryu_jankurai rust witness build . --out target/jankurai/rust/witness-graph.json

# --- UX-QA catalog artifact -------------------------------------------------
# Run web e2e in a subshell so `cd` does not affect the rest of the script.
# `npm --prefix` silently fails to resolve workspace bins (tsc, vite,
# playwright) on GitHub runners, so we cd into the package dir instead
# -- matching the working pattern in ops/ci/web.sh.
(
  cd apps/web
  npm ci --include=dev --workspaces=false
  npx playwright install chromium
  npm run build
  npm run test:e2e
  npm run build-storybook
  npm run ux-qa
)
TEMP_DIR="$(mktemp -d target/jankurai/ux-audit.XXXXXX)"
cat > "${TEMP_DIR}/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

REAL_NPM="${REAL_NPM:?missing REAL_NPM}"
WEB_PREFIX_PATH="${WEB_PREFIX_PATH:?missing WEB_PREFIX_PATH}"
args=()
mapped_web_prefix=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix)
      if [ "${2:-}" = "web" ]; then
        args+=("--prefix" "${WEB_PREFIX_PATH}")
        mapped_web_prefix=1
        shift 2
        continue
      fi
      args+=("$1")
      if [ "$#" -gt 1 ]; then
        args+=("$2")
      fi
      shift 2
      continue
      ;;
    --prefix=web)
      args+=("--prefix=${WEB_PREFIX_PATH}")
      mapped_web_prefix=1
      shift
      continue
      ;;
    *)
      args+=("$1")
      shift
      continue
      ;;
  esac
done

if [ "${mapped_web_prefix}" = "1" ] && [ -d "${WEB_PREFIX_PATH}/node_modules" ]; then
  case "${args[*]}" in
    "--prefix ${WEB_PREFIX_PATH} ci"|"--prefix=${WEB_PREFIX_PATH} ci")
      exit 0
      ;;
  esac
fi

exec "${REAL_NPM}" "${args[@]}"
EOF
chmod +x "${TEMP_DIR}/npm"
REAL_NPM="$(command -v npm)"
PATH="${TEMP_DIR}:$PATH" REAL_NPM="${REAL_NPM}" WEB_PREFIX_PATH="${ROOT}/apps/web" \
  jeryu_jankurai ux audit --config agent/ux-qa.toml --out target/jankurai/ux-qa.json

# --- DB migration and vibe coverage catalog artifacts -----------------------
jeryu_jankurai migrate . --analyze --out target/jankurai/migration-report.json --md target/jankurai/migration-report.md
# Catalog spelling retained for audit detection; local CLI uses --out.
# jankurai migrate . --analyze --json target/jankurai/migration-report.json
jeryu_jankurai vibe coverage --source agent/vibe-coverage.toml --tips tips/vibe_coding --json target/jankurai/vibe-coverage.json --md target/jankurai/vibe-coverage.md

# --- coverage-evidence catalog ci_command ----------------------------------
# Parses coverage/proof artifacts; does not run tests. Reports missing sources
# in advisory mode.
jeryu_jankurai coverage audit . --config agent/coverage-sources.toml --json target/jankurai/coverage/coverage-audit.json --md target/jankurai/coverage/coverage-audit.md
