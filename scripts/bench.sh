#!/usr/bin/env bash
# Run sift-core Criterion benchmarks (statistical: 150 samples, 10s measurement window per case).
# Examples:
#   ./scripts/bench.sh
#   ./scripts/bench.sh -- --save-baseline main
#   ./scripts/bench.sh -- --baseline main
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
exec cargo bench -p sift-core --bench search "$@"
