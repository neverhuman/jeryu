#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${CI_JOB_TOKEN:-}" ]]; then
  echo "error: CI_JOB_TOKEN is not available; this doctor must run inside GitLab CI" >&2
  exit 2
fi
if [[ -z "${CI_SERVER_URL:-}" ]]; then
  echo "error: CI_SERVER_URL is not available" >&2
  exit 2
fi

project_path="${JERYU_CI_JOB_TOKEN_PROBE_PROJECT_PATH:-${CI_PROJECT_PATH:-}}"
if [[ -z "$project_path" ]]; then
  echo "error: set JERYU_CI_JOB_TOKEN_PROBE_PROJECT_PATH or CI_PROJECT_PATH" >&2
  exit 2
fi

base="${CI_SERVER_URL%/}"
case "$base" in
  http://*|https://*) probe_url="${base}/${project_path}.git" ;;
  *)
    echo "error: unsupported CI_SERVER_URL scheme" >&2
    exit 2
    ;;
esac

askpass="$(mktemp)"
trap 'rm -f "$askpass"' EXIT
cat >"$askpass" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  *Username*) printf '%s\n' "${GIT_USERNAME:?}" ;;
  *) printf '%s\n' "${GIT_PASSWORD:?}" ;;
esac
EOF
chmod 700 "$askpass"

if GIT_ASKPASS="$askpass" \
  GIT_TERMINAL_PROMPT=0 \
  GIT_USERNAME=gitlab-ci-token \
  GIT_PASSWORD="$CI_JOB_TOKEN" \
  git -c credential.helper= ls-remote --exit-code "$probe_url" HEAD >/dev/null 2>&1; then
  echo "CI_JOB_TOKEN source clone doctor ok for ${project_path}"
else
  echo "error: CI_JOB_TOKEN cannot read ${project_path}; fix GitLab job-token clone permissions before high-value pipelines run" >&2
  exit 1
fi
