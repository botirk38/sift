# sift reference

## Commands

| Command | Purpose |
|---------|---------|
| `sift update` | Upgrade the installed **binary** (latest GitHub release) |
| `sift index build [PATH]` | Create an index (async via daemon by default; fails if one already exists) |
| `sift index build --wait [PATH]` | Blocking build (waits until complete) |
| `sift index update [PATH]` | Full rebuild of an existing index (async by default; not incremental) |
| `sift index update --wait [PATH]` | Blocking full rebuild |
| `sift PATTERN [PATH...]` | Search (indexed or walk mode) |

## Common flags

| Flag | Purpose |
|------|---------|
| `-i` | Case-insensitive |
| `-w` | Whole word |
| `-F` | Fixed string (no regex) |
| `-c` | Count matches per file |
| `-l` | List matching files only |
| `-L` / `--follow` | Follow symlinks (index and search) |
| `-g GLOB` | Filter paths by glob |
| `-A` / `-B` / `-C` | Context lines |
| `--json` | JSON Lines output |
| `--stats` | Summary on stderr |
| `--debug` | Search diagnostics on stderr (sift-dir, index, mode, candidates) |
| `-0` / `--null` | NUL-separated paths |
| `--no-filename` | Omit path prefix (not `-h`) |
| `-j N` / `--threads N` | Rayon thread count |
| `--sift-dir DIR` | Index directory (default `.sift`) |

Patterns: positional, or `-e PATTERN`, or `-f FILE`. Multiple patterns are OR’d unless configured otherwise.

## Index

```bash
sift --sift-dir .sift index build .
sift --sift-dir .sift index build --wait .
sift --sift-dir .sift index update .
sift --sift-dir .sift index update --wait .
sift --sift-dir .sift index build --index ngram --width 3 .
```

- `index build` and `index update` are both async via daemon by default; use `--wait` for blocking.
- `index update` fully rebuilds the snapshot (same pipeline as build); it does not apply an incremental delta.
- Search may queue background indexing for unindexed hit paths (when the daemon is enabled).
- With the daemon running, filesystem watches refresh the index after corpus changes.
- `PATH` defaults to `.` and must be a **directory**.
- Repeatable `--index KIND` selects kinds to build (default: ngram width 3). Kinds: `ngram` (optional `--width` / `--norm` after it) and `ast` (tree-sitter node-kind index over rust/python/javascript/typescript/tsx/go/java; opt-in, powers upcoming structural search).
- Search paths must lie under the indexed **corpus root** when an index exists.

## Binary upgrade

```bash
sift update
# or
curl -fsSL https://raw.githubusercontent.com/botirk38/sift/master/scripts/install.sh | sh
```

Installs both `sift` and `sift-daemon` to `$PREFIX/bin` (default `$HOME/.local/bin`). Background indexing requires `sift-daemon` as a sibling of `sift` on PATH.

Environment: `SIFT_REPO`, `SIFT_VERSION`, `PREFIX`, `BIN_DIR` (same as install.sh).

## Daemon

After `index build`, `index update`, or search, sift may spawn `sift-daemon` to reconcile index work over IPC and refresh on filesystem changes. Keep the daemon enabled for normal interactive and agent use. Prefer `--wait` on index commands when you need the rebuild to finish before the next search.

`SIFT_NO_DAEMON=1` disables daemon spawn when a background process is unavailable (for example constrained CI). It is not required for ordinary workflows.

The daemon prefers the native filesystem watcher from `notify` (`RecommendedWatcher`). If that backend cannot start, it falls back to polling and logs a warning. On macOS this commonly happens under seatbelt sandboxes that deny Mach lookup of `com.apple.FSEvents` (for example Cursor’s default agent sandbox).

## Limitations

- Requires `sift index build` for indexed speedup.
- PCRE2 is available via `-P` / `--pcre2` (disables index narrowing).
- Patterns without indexable literals may full-scan at roughly ripgrep speed.
- Not a replacement for `git log` / history search.

## Differences from ripgrep

| Topic | ripgrep | sift |
|-------|---------|------|
| Index | None | `.sift` via `index build` |
| `-h` | No filename | Help |
| Tool upgrade | Package manager | `sift update` |

Flag/search UX aims for ripgrep parity; remaining gaps are tracked as GitHub issues.

## Install from source

```bash
cargo build --release -p sift-grep
```

Produces `sift` and `sift-daemon` (package `sift-grep` on crates.io). Background indexing requires `sift-daemon` on PATH beside `sift`.
