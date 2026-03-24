#!/usr/bin/env bash
# Profile sift-core via the `sift-profile` binary (release + line tables: workspace `profiling` profile).
#
#   ./scripts/profile.sh list                    # list all available scenarios
#   ./scripts/profile.sh literal                  # profile single scenario (default 15s loop)
#   ./scripts/profile.sh word_literal            # word-boundary wrapped literal
#   ./scripts/profile.sh casei_literal          # case-insensitive literal
#   ./scripts/profile.sh required_literal       # mixed regex like [A-Z]+_RESUME
#   ./scripts/profile.sh unicode_class          # Unicode category
#   ./scripts/profile.sh no_literal             # no-literal regex
#   ./scripts/profile.sh alternation            # literal alternation
#   ./scripts/profile.sh alternation_casei     # case-insensitive alternation
#   ./scripts/profile.sh line_regexp            # whole-line regex
#   ./scripts/profile.sh fixed_string          # fixed string (-F)
#   ./scripts/profile.sh build                  # index build benchmark
#
#   ./scripts/profile.sh flamegraph literal     # flamegraph a single scenario (~30s)
#   ./scripts/profile.sh flamegraph build      # flamegraph build
#
#   SIFT_LARGE=1 ./scripts/profile.sh literal  # use large corpus (~8k files)
#   SIFT_CORPUS_FILES=20000 ./scripts/profile.sh no_literal
#   SIFT_LOOP_SECS=30 ./scripts/profile.sh required_literal
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
    scenario="${second:-literal}"
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
