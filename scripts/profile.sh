#!/usr/bin/env bash
# Profile sift-core via the `sift-profile` binary (release + line tables: workspace `profiling` profile).
#
#   ./scripts/profile.sh                    # metrics, mode narrow
#   ./scripts/profile.sh full_dotstar       # metrics shorthand (mode only)
#   ./scripts/profile.sh metrics narrow
#   ./scripts/profile.sh flamegraph narrow  # ~30s capture; writes flamegraph.svg
#
# Env: SIFT_ITERS, SIFT_LOOP_SECS, SIFT_LARGE, SIFT_CORPUS_FILES, SIFT_CORPUS_LINES, SIFT_CORPUS_DIRS
# (flamegraph sets SIFT_LOOP_SECS=30 unless set; cleared for `build`).
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

cargo_prof=(--profile profiling -p sift-core --features profile --bin sift-profile)

first="${1:-metrics}"
second="${2:-}"

case "$first" in
metrics | flamegraph)
	cmd="$first"
	mode="${second:-narrow}"
	;;
narrow | full_dotstar | full_ci | build)
	cmd="metrics"
	mode="$first"
	;;
*)
	cmd="metrics"
	mode="${first:-narrow}"
	;;
esac

if [[ "$cmd" == flamegraph ]]; then
	if [[ "$mode" == build ]]; then
		unset SIFT_LOOP_SECS || true
	else
		export SIFT_LOOP_SECS="${SIFT_LOOP_SECS:-30}"
	fi
	exec cargo flamegraph "${cargo_prof[@]}" -- "$mode"
fi

exec cargo run "${cargo_prof[@]}" -- "$mode"
