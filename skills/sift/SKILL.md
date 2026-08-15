---
name: sift
description: >-
  Search codebases with the sift CLI using indexed grep. Builds an N-gram
  index (trigram by default) once, then runs ripgrep-compatible queries 2 to
  3x faster by skipping irrelevant files. Use when exploring a repository,
  finding symbols or patterns, grepping across a large codebase, or when the
  user mentions sift, indexed search, or .sift. Also use when the user wants
  faster grep results or is searching a repo with more than a few thousand
  files. Not for developing sift itself (crates/cli, sift-core, clap, cargo
  test).
metadata:
  author: botirk38
  version: "1.1.0"
  tags: grep, search, index, ngram, trigram, ripgrep, code-search
allowed-tools: Read Grep Bash(sift:*) Bash(rg:*)
---

# sift

Indexed grep for codebases. Build an index once, then search with ripgrep-like flags. Without an index, sift falls back to a slower directory walk.

## When to use

- Searching a repository for patterns, symbols, or strings
- Exploring unfamiliar codebases (find usages, definitions, imports)
- The user mentions sift, indexed search, `.sift`, or wants faster grep
- Any repo with more than a few thousand files where grep speed matters

Do not use this skill for developing or debugging sift itself. Use the repository's `AGENTS.md` for that.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/botirk38/sift/master/scripts/install.sh | sh
sift --version
```

Upgrade: `sift update` (or re-run the install script).

Do not `cargo build` unless the user is in the sift source tree and asked to build from source.

## How it works

1. Build an N-gram index (default width 3 / trigrams) for every file under the corpus root
2. At query time, decompose the pattern into N-gram terms and intersect posting lists
3. Search only candidate files with the full regex engine

```bash
cd /path/to/repo
sift --sift-dir .sift index build --wait .   # one-time, blocking
sift "pattern" [PATH...]                     # indexed search
sift -l "pattern"                            # list matching files
sift -F "literal.string"                     # fixed string (no regex)
```

After large changes, rebuild: `sift --sift-dir .sift index update --wait .`  
(`index update` is a full rebuild of an existing index; it is not incremental.)

With the daemon enabled (default), filesystem watches also refresh the index in the background.

## Workflow

```text
1. cd to repository root
2. Check for .sift/ directory (or confirm --sift-dir location)
3. If no index: sift --sift-dir .sift index build --wait .
4. After large repo changes: sift --sift-dir .sift index update --wait .
5. Narrow with sift -l "pattern" [PATH...]
6. Full search; use -F for literals with regex metacharacters
7. Use --json only when parsing output programmatically
```

## Indexed vs walk mode

**Index present** (`.sift` directory with a built snapshot): fast N-gram narrowing, searches only candidate files. Search paths must be under the indexed corpus root (a directory).

**No index**: walk mode from cwd only, comparable to scanning without indexing. Always `cd` to the repo root and run `index build --wait` before serious exploration.

## Rules

- Global `--sift-dir` goes before subcommands: `sift --sift-dir .sift index build .`
- `index build` creates an index (fails if one already exists); `index update` fully rebuilds an existing one. Both are async by default; `--wait` blocks
- Corpus path for index commands must be a **directory**
- `sift update` upgrades the binary, not the index
- Rust `regex` syntax by default; `-F` for fixed strings
- To search for a pattern that matches a subcommand name: `sift -- index` or `sift -e index`
- `-h` is help (not "no filename"); use `--no-filename` instead
- Leave the daemon enabled for normal use. Prefer `index build --wait` / `index update --wait` when you need a blocking rebuild. `SIFT_NO_DAEMON=1` is an escape hatch for environments that cannot run a background daemon (for example some CI images), not the default agent workflow.

## Additional resources

- [reference.md](reference.md): all flags, daemon details, limitations, rg differences
- [README.md](../../README.md): user quick start
