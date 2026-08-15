# rg Compatibility Matrix

This tracks `sift` against `rg` with **no backward compatibility**: when a sift flag
conflicts with ripgrep, `rg` wins. The current target is full parity for
search UX; PCRE2 is available via `--pcre2` (see row below).

Use the local `ripgrep/` clone as the source of truth when updating a row, and
back each implemented row with `sift`-only golden tests. Do not spawn `rg` at runtime.

| Area | `rg` reference | `sift` status | Notes / next test |
|---|---|---|---|
| `-L` / `--follow` | `ripgrep/crates/core/flags/defs.rs`, `ripgrep/tests/misc.rs` (`symlink_follow`) | Implemented | Traversal matches `rg`; add path-output tests with absolute/relative scopes. |
| `--files-without-match` | `ripgrep/crates/core/flags/defs.rs`, `ripgrep/tests/misc.rs` (`files_without_match`) | Implemented | Long flag only, no `-L`; output matches `rg` for relative and absolute scopes. |
| Default path printing | `ripgrep/crates/printer/src/standard.rs`, `ripgrep/tests/misc.rs` | Implemented | Respects user scopes: absolute scopes produce absolute paths, relative scopes produce relative paths. Unified through `PathDisplay` in core. |
| `--heading` / `--no-heading` | `ripgrep/crates/printer/src/standard.rs`, `ripgrep/tests/misc.rs` (`with_heading`) | Implemented | See `integration_output.rs`. |
| Ignore / git defaults | `ripgrep/crates/ignore`, `ripgrep/crates/core/flags/defs.rs` | Implemented | Full `--no-ignore*` / `--ignore*` family with last-wins toggles; `--ignore-file`; `-u`/`--unrestricted` (1× disable ignore, 2× include hidden). Golden tests in `integration_ignore.rs` (walk + index for `-u`, `--no-ignore-dot`, `--no-ignore-vcs`, `--no-ignore-exclude`, `--ignore-file`). `--no-ignore-global` only smoke-tested (needs global gitignore fixture). |
| Context output (`-A/-B/-C`) | `ripgrep/crates/core/flags/defs.rs`, `ripgrep/tests/misc.rs` (`after_context`) | Implemented | See `integration_context.rs`. |
| Color / separators / null | `ripgrep/crates/printer/src/standard.rs`, `ripgrep/crates/printer/src/summary.rs` | Implemented | `--color`, `-0` / `--null`; see `integration_null_color.rs`. |
| `--stats` | `ripgrep/crates/printer/stats.rs` (approx.) | Partial | Stderr lines: match tally (line-level, not per-regex occurrence), files contained matches, files searched, bytes printed, bytes searched (metadata sum), elapsed (index + walk). Golden tests in `integration_stats.rs`. Index mode `files searched` counts trigram candidates, not full corpus. **Deferred:** rg per-occurrence match count on multi-match lines, "matched lines" stat line, split search/print timing, exact byte totals vs rg. |
| Encoding / multiline | `ripgrep/crates/core/flags/defs.rs`, `ripgrep/tests/json.rs`, `ripgrep/tests/multiline.rs` | Implemented | `-U`/`--multiline`, `--multiline-dotall`, `--crlf`; index/walk consistency for multiline, dotall, and CRLF. Golden tests in `integration_threading.rs`. Encoding flags (`-E`, `--encoding`, …) deferred. |
| `--json` | `ripgrep/tests/json.rs`, `grep-printer` JSON | Implemented | JSON Lines (`begin` / `match` / `context` / `end` / `summary`). Human `--stats` lines are opt-in via `--stats` (match current `rg`). See `integration_json.rs`. |
| `--vimgrep` | `ripgrep/tests/misc.rs` (`vimgrep`) | Implemented | Implies line numbers and column; index/walk parity. Golden tests in `integration_output.rs`. |
| `--debug` | `ripgrep/crates/core/flags/defs.rs`, logging throughout search | Deferred | Flag not wired; `--debug` exits 2 (unknown flag). Large scope: structured debug logging across matcher, ignore, index, and worker paths. |
| `-P` / PCRE2 | `ripgrep/crates/pcre2`, `ripgrep/crates/core/flags/defs.rs` | Partial | `--pcre2` / `--engine pcre2` work; short `-P` alias tracked separately. PCRE2 disables index narrowing. |

## Implementation Notes

- Path display is resolved in CLI based on user-provided scopes only.
- Core receives resolved `PathDisplay` through `GrepLineStyle`.
- All output paths (standard, heading, summary) use `display_path_for_candidate()`.
- No runtime `rg` dependency in tests; use `ripgrep/` clone as manual reference only.
