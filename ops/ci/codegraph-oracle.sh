#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

OUT_DIR="target/jankurai/codegraph-oracle"
DB_PATH="$OUT_DIR/codegraph.sqlite"
IMPACT_PACK="$OUT_DIR/impact-pack.json"
RECEIPT="$OUT_DIR/receipt.json"
mkdir -p "$OUT_DIR"
rm -f "$IMPACT_PACK" "$RECEIPT"

run_step() {
  printf '▶ %s\n' "$1"
  shift
  "$@"
}

commit_sha="$(git rev-parse HEAD)"
ref_name="$(git rev-parse --abbrev-ref HEAD)"

run_step "codegraph tests" cargo test -p jeryu-codegraph --jobs 40
run_step "api codegraph tests" cargo test -p jeryu-api --features web --jobs 40 codegraph
run_step "mcp codegraph tests" cargo test -p jeryu-mcp --jobs 40 codegraph_query
printf '▶ %s\n' "codegraph query"
cargo run -q -p jeryu-codegraph -- query \
  --root . \
  --db "$DB_PATH" \
  --repo-id jeryu \
  --owner neverhuman \
  --name jeryu \
  --ref-name "$ref_name" \
  --commit-sha "$commit_sha" \
  > "$IMPACT_PACK"

python3 - "$IMPACT_PACK" "$RECEIPT" "$commit_sha" "$ref_name" "$DB_PATH" <<'PY'
import hashlib
import json
import pathlib
import sys

impact_path = pathlib.Path(sys.argv[1])
receipt_path = pathlib.Path(sys.argv[2])
commit_sha = sys.argv[3]
ref_name = sys.argv[4]
db_path = pathlib.Path(sys.argv[5])

impact = json.loads(impact_path.read_text())
receipt = impact.get("receipt", {})
if receipt.get("storage_backend") != "sqlite":
    raise SystemExit("impact pack missing sqlite storage backend")
if receipt.get("schema_version") != 1:
    raise SystemExit("impact pack missing schema version 1")

def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8192), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"

payload = {
    "schema": "jeryu.codegraph-oracle.receipt/v1",
    "commands": [
        "cargo test -p jeryu-codegraph --jobs 40",
        "cargo test -p jeryu-api --features web --jobs 40 codegraph",
        "cargo test -p jeryu-mcp --jobs 40 codegraph_query",
        f"cargo run -q -p jeryu-codegraph -- query --root . --db target/jankurai/codegraph-oracle/codegraph.sqlite --repo-id jeryu --owner neverhuman --name jeryu --ref-name {ref_name} --commit-sha {commit_sha}",
    ],
    "exit_status": 0,
    "commit": commit_sha,
    "ref_name": ref_name,
    "schema_version": receipt.get("schema_version"),
    "schema_digest": receipt.get("schema_digest"),
    "artifact_paths": {
        "impact_pack": str(impact_path),
        "database": str(db_path),
    },
    "digests": {
        "impact_pack": sha256(impact_path),
        "database": sha256(db_path),
    },
}
receipt_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY

printf 'codegraph oracle artifacts: %s, %s\n' "$IMPACT_PACK" "$RECEIPT"
