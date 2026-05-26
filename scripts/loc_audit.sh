#!/bin/sh
# Audit src/tui Rust files against the reset file-size budget.

set -eu

WARN_LIMIT=300
FAIL_LIMIT=350
ROOT="src/tui"
ROOT_FILE="src/tui.rs"

usage() {
    printf 'Usage: %s [--warn N] [--fail N]\n' "$0" >&2
}

is_uint() {
    case "$1" in
        ''|*[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --warn)
            [ "$#" -ge 2 ] || {
                usage
                exit 2
            }
            WARN_LIMIT="$2"
            shift 2
            ;;
        --warn=*)
            WARN_LIMIT=${1#--warn=}
            shift
            ;;
        --fail)
            [ "$#" -ge 2 ] || {
                usage
                exit 2
            }
            FAIL_LIMIT="$2"
            shift 2
            ;;
        --fail=*)
            FAIL_LIMIT=${1#--fail=}
            shift
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

if ! is_uint "$WARN_LIMIT" || ! is_uint "$FAIL_LIMIT"; then
    printf 'error: --warn and --fail must be non-negative integers\n' >&2
    exit 2
fi

if [ "$WARN_LIMIT" -gt "$FAIL_LIMIT" ]; then
    printf 'error: --warn must be less than or equal to --fail\n' >&2
    exit 2
fi

if [ ! -d "$ROOT" ]; then
    printf 'error: %s is not a directory\n' "$ROOT" >&2
    exit 2
fi

printf 'TUI LOC budget: warn > %s, fail > %s\n' "$WARN_LIMIT" "$FAIL_LIMIT"
printf '%7s  %-6s  %s\n' "LOC" "STATUS" "PATH"
printf '%7s  %-6s  %s\n' "-------" "------" "----"

{
    if [ -f "$ROOT_FILE" ]; then
        loc=$(wc -l < "$ROOT_FILE")
        printf "%s\t%s\n" "$loc" "$ROOT_FILE"
    fi

    find "$ROOT" -type f -name '*.rs' -exec sh -c '
        for path do
            loc=$(wc -l < "$path")
            printf "%s\t%s\n" "$loc" "$path"
        done
    ' sh {} +
} |
LC_ALL=C sort -t "$(printf '\t')" -k1,1nr -k2,2 |
awk -F '\t' -v warn="$WARN_LIMIT" -v fail="$FAIL_LIMIT" '
    {
        loc = $1 + 0
        path = $2
        status = "OK"
        if (loc > fail) {
            status = "FAIL"
            failed = 1
        } else if (loc > warn) {
            status = "WARN"
        }
        printf "%7d  %-6s  %s\n", loc, status, path
    }
    END {
        if (failed) {
            exit 1
        }
    }
'
