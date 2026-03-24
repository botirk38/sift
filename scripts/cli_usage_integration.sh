#!/usr/bin/env bash
# Exercise the sift binary with varied argv (exit codes must be 0, 1, or 2 only).
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
cargo build -p sift-cli -q
SIFT="${CARGO_TARGET_DIR:-$repo_root/target}/debug/sift"

tdir="${TMPDIR:-/tmp}"
tmp=$(mktemp -d "${tdir}/sift-usage-integration.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT

mkdir -p "$tmp/corpus/a" "$tmp/corpus/b"
printf '%s\n' 'hello world' 'line two' >"$tmp/corpus/a/x.txt"
printf '%s\n' 'baz' 'quux' >"$tmp/corpus/b/y.txt"

idx="$tmp/idx"

"$SIFT" --index "$idx" build "$tmp/corpus"

assert_exit_ok() {
  local code=$1 msg=$2
  if [[ "$code" -ne 0 && "$code" -ne 1 && "$code" -ne 2 ]]; then
    printf 'unexpected exit %s: %s\n' "$code" "$msg" >&2
    exit 1
  fi
}

run() {
  local code
  set +e
  "$@"
  code=$?
  set -e
  assert_exit_ok "$code" "$*"
}

# Search variants (flags before --index as in tests)
run "$SIFT" --index "$idx" "hello"
run "$SIFT" -q --index "$idx" "nope_not_found"
run "$SIFT" --index "$idx" -i -F "HELLO"
run "$SIFT" --index "$idx" -c "hello"
run "$SIFT" --index "$idx" -l "hello"
run "$SIFT" --index "$idx" -L "ZZZ_NO_SUCH_LINE"
run "$SIFT" --index "$idx" --no-filename "world"
run "$SIFT" --index "$idx" --max-count 1 "line"
run "$SIFT" -e "hello" -e "baz" --index "$idx"

# Path scoping (cwd = corpus root)
(
  cd "$tmp/corpus"
  run "$SIFT" --index "$idx" "hello" a
  run "$SIFT" --index "$idx" -c "." a b
)

# Missing pattern → clap / app error (exit 2)
set +e
"$SIFT" --index "$idx" 2>/dev/null
code=$?
set -e
assert_exit_ok "$code" "missing pattern"

printf 'cli_usage_integration: ok\n'
