#!/usr/bin/env bash
set -euo pipefail

cargo test -p jeryu-bench baseline_replay
