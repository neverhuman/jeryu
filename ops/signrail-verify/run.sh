#!/usr/bin/env bash
set -euo pipefail
cargo test -p jeryu-signrail --test release_witness
