#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cd "$REPO_ROOT"
require_tool jq

grep -Fqx -- '- `tool-manifest.toml` is the immutable Jankurai pin and consumer-rendering authority.' contracts/README.md
grep -Fqx -- '- `tools-registry.toml` and `tasks/*.toml` are the reusable-tool and build-queue authorities.' contracts/README.md
grep -Fqx -- '- `schemas/*.schema.json` are the closed evidence and repair receipt contracts.' contracts/README.md

for schema in schemas/*.schema.json; do
  jq -e '."$schema" == "https://json-schema.org/draft/2020-12/schema" and
    .type == "object" and .additionalProperties == false' "$schema" >/dev/null
done
jq -e '.properties.schema_version.const == "jeryu.tool.repair-receipt/v1" and
  .properties.repo.const == "jeryu/jeryu-tool"' \
  schemas/repair-receipt.schema.json >/dev/null
jq -e '.required == ["path", "priority", "task", "why"] and
  (.properties | keys) == ["lane", "owner", "path", "priority", "rule_id", "task", "tlr", "why"]' \
  schemas/repair-queue.schema.json >/dev/null
jq -e '.properties.repository.const == "jeryu/jeryu-tool" and
  .properties.publication_performed.const == false and
  .properties.installation_performed.const == false' \
  schemas/artifact-support.schema.json >/dev/null
jq -e '.properties.schema_version.const == "jeryu.tool.security/v1"' \
  schemas/security-evidence.schema.json >/dev/null
bash ops/render-tool-manifest.sh --check --repo jeryu-tool
bash ops/registry-summary.sh --check >/dev/null
printf 'contract drift ok: closed schemas, manifest pin, and registry\n'
