#!/usr/bin/env bash
# Run cargo-fuzz from the fuzz/ crate so fuzz/rust-toolchain.toml (nightly) applies.
#
# Usage:
#   scripts/fuzz.sh run search_usage -- -runs=1000
#   scripts/fuzz.sh quick                    # ~20s wall time, search_usage
#   scripts/fuzz.sh quick compile_only       # ~20s wall time, named target
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root/fuzz"

if [[ "${1:-}" == quick ]]; then
  shift
  target="${1:-search_usage}"
  [[ $# -gt 0 ]] && shift
  exec cargo fuzz run "$target" -- -max_total_time=20 "$@"
fi

exec cargo fuzz "$@"
