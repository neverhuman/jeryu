#!/usr/bin/env bash
set -euo pipefail
cargo test -p signrail --test release_witness
