#!/usr/bin/env bash
# Sync or verify README.md against a Jankurai repo-score JSON report.

set -euo pipefail

mode="update"
score_json="agent/repo-score.json"
readme="README.md"

usage() {
  cat >&2 <<'USAGE'
Usage: bash scripts/sync-readme-jankurai-score.sh [--update|--check] [--score-json PATH] [--readme PATH]
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --update)
      mode="update"
      shift
      ;;
    --check)
      mode="check"
      shift
      ;;
    --score-json)
      [ "$#" -ge 2 ] || {
        usage
        exit 2
      }
      score_json="$2"
      shift 2
      ;;
    --readme)
      [ "$#" -ge 2 ] || {
        usage
        exit 2
      }
      readme="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf 'missing required tool: %s\n' "$1" >&2
    exit 127
  fi
}

require_tool jq
require_tool perl

[ -f "$score_json" ] || {
  printf 'score JSON not found: %s\n' "$score_json" >&2
  exit 2
}
[ -f "$readme" ] || {
  printf 'README not found: %s\n' "$readme" >&2
  exit 2
}

jq_number() {
  local expr="$1"
  jq -er "$expr | if type == \"number\" and floor == . then tostring else error(\"expected integer number\") end" "$score_json"
}

jq_string() {
  local expr="$1"
  jq -er "$expr | if type == \"string\" then . else error(\"expected string\") end" "$score_json"
}

jq_bool() {
  local expr="$1"
  jq -er "$expr | if type == \"boolean\" then tostring else error(\"expected boolean\") end" "$score_json"
}

score="$(jq_number '.score')"
minimum_score="$(jq_number '.decision.minimum_score')"
decision_status="$(jq_string '.decision.status')"
decision_passed="$(jq_bool '.decision.passed')"
auditor_version="$(jq_string '.auditor_version')"
observed_level="$(jq_string '.observed_conformance_level')"

for value in "$decision_status" "$auditor_version" "$observed_level"; do
  if [[ ! "$value" =~ ^[A-Za-z0-9_.-]+$ ]]; then
    printf 'unexpected score metadata value: %s\n' "$value" >&2
    exit 2
  fi
done

if [ "$score" -lt 0 ] || [ "$score" -gt 100 ]; then
  printf 'score out of range: %s\n' "$score" >&2
  exit 2
fi

if [ "$decision_passed" = "true" ] && [ "$score" -ge 90 ]; then
  badge_color="brightgreen"
elif [ "$decision_passed" = "true" ] && [ "$score" -ge "$minimum_score" ]; then
  badge_color="green"
elif [ "$score" -ge 70 ]; then
  badge_color="yellow"
else
  badge_color="red"
fi

badge_alt="jankurai score $score"
badge_src="https://img.shields.io/badge/jankurai-$score-$badge_color"
score_block="$(cat <<BLOCK
  <!-- jankurai-score:start -->
  <p><strong>Jankurai score:</strong> <a href="agent/repo-score.md"><code>${score}/100</code></a> (${decision_status}, minimum ${minimum_score}, ${observed_level}, auditor ${auditor_version})</p>
  <!-- jankurai-score:end -->
BLOCK
)"

tmp="$(mktemp)"
cleanup() {
  rm -f "$tmp"
}
trap cleanup EXIT

cp "$readme" "$tmp"

JANKURAI_BADGE_ALT="$badge_alt" \
JANKURAI_BADGE_SRC="$badge_src" \
perl -0pi -e '
  my $alt = $ENV{"JANKURAI_BADGE_ALT"};
  my $src = $ENV{"JANKURAI_BADGE_SRC"};
  my $count = s{(<img\s+alt=")jankurai score [0-9]+("[^>]*\s+src=")https://img\.shields\.io/badge/jankurai-[0-9]+-[A-Za-z0-9_-]+(")}{$1$alt$2$src$3}g;
  die "README.md must contain exactly one jankurai score badge\n" unless $count == 1;
' "$tmp"

if grep -q '<!-- jankurai-score:start -->' "$tmp"; then
  README_SCORE_BLOCK="$score_block" \
  perl -0pi -e '
    my $block = $ENV{"README_SCORE_BLOCK"};
    my $count = s{[ \t]*<!-- jankurai-score:start -->.*?[ \t]*<!-- jankurai-score:end -->}{$block}s;
    die "README.md must contain exactly one jankurai score block\n" unless $count == 1;
  ' "$tmp"
else
  README_SCORE_BLOCK="$score_block" \
  perl -0pi -e '
    my $block = $ENV{"README_SCORE_BLOCK"};
    my $count = s{(  </p>\n)(  <h3>)}{$1$block\n$2}s;
    die "README.md badge section did not match expected insertion point\n" unless $count == 1;
  ' "$tmp"
fi

if cmp -s "$readme" "$tmp"; then
  printf 'README.md jankurai score is current (%s/100)\n' "$score"
  exit 0
fi

if [ "$mode" = "check" ]; then
  printf 'README.md jankurai score is stale; run `just score` locally.\n' >&2
  diff -u "$readme" "$tmp" >&2 || true
  exit 1
fi

mv "$tmp" "$readme"
trap - EXIT
printf 'Updated README.md jankurai score to %s/100\n' "$score"
