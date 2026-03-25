#!/usr/bin/env bash
# Profile sift-core via the `sift-profile` binary (release + line tables: workspace `profiling` profile).
#
# Query-planning scenarios:
#   ./scripts/profile.sh list
#   ./scripts/profile.sh literal_narrow
#   ./scripts/profile.sh word_literal
#   ./scripts/profile.sh line_literal
#   ./scripts/profile.sh fixed_string
#   ./scripts/profile.sh casei_literal
#   ./scripts/profile.sh smart_case_lower
#   ./scripts/profile.sh smart_case_upper
#   ./scripts/profile.sh required_literal
#   ./scripts/profile.sh no_literal
#   ./scripts/profile.sh alternation
#   ./scripts/profile.sh alternation_casei
#   ./scripts/profile.sh unicode_class
#
# Filter + query scenarios:
#   ./scripts/profile.sh glob_include
#   ./scripts/profile.sh glob_exclude
#   ./scripts/profile.sh glob_casei
#   ./scripts/profile.sh hidden_default
#   ./scripts/profile.sh hidden_include
#   ./scripts/profile.sh ignore_default
#   ./scripts/profile.sh ignore_custom
#   ./scripts/profile.sh scoped_search
#
# Output-mode scenarios:
#   ./scripts/profile.sh only_matching
#   ./scripts/profile.sh count
#   ./scripts/profile.sh count_matches
#   ./scripts/profile.sh files_with_matches
#   ./scripts/profile.sh files_without_match
#   ./scripts/profile.sh max_count_1
#
# Build benchmark:
#   ./scripts/profile.sh build
#
# Large corpus:
#   SIFT_LARGE=1 ./scripts/profile.sh literal_narrow
#   SIFT_CORPUS_FILES=20000 ./scripts/profile.sh no_literal
#
# Timing control:
#   SIFT_LOOP_SECS=30 ./scripts/profile.sh required_literal
#
# Flamegraph:
#   ./scripts/profile.sh flamegraph literal_narrow
#   ./scripts/profile.sh flamegraph build
#
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

cargo_prof=(--profile profiling -p sift-core --features profile --bin sift-profile)

first="${1:-list}"
second="${2:-}"

case "$first" in
list)
    exec cargo run "${cargo_prof[@]}" -- list
    ;;
flamegraph)
    scenario="${second:-literal_narrow}"
    if [[ "$scenario" == build ]]; then
        unset SIFT_LOOP_SECS || true
    else
        export SIFT_LOOP_SECS="${SIFT_LOOP_SECS:-30}"
    fi
    exec cargo flamegraph "${cargo_prof[@]}" -- "$scenario"
    ;;
*)
    exec cargo run "${cargo_prof[@]}" -- "$first"
    ;;
esac
