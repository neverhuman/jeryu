#!/usr/bin/env bash
# Run the 256 retained policy/profile fixtures against the common Rust gate.
# Seven unchanged component Python bodies are not executed by this transport.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"
cargo test --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split \
  audit_score::tests::legacy_policy_case_matrix -- --exact --nocapture | awk '
  { print }
  $0 == "256 score policy/report cases passed" { cases++ }
  /^test result: ok[.] 1 passed; 0 failed; 0 ignored; / { summaries++ }
  END {
    if (cases != 1 || summaries != 1) {
      print "score policy regression did not execute its exact required case set" > "/dev/stderr"
      exit 1
    }
  }
'
