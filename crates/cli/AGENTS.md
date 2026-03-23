# Agent notes (sift-cli)

Installable agent skill ([skills.sh](https://skills.sh) / `npx skills`): [`skills/sift-cli/SKILL.md`](../../skills/sift-cli/SKILL.md).

## Structure

- **`src/main.rs`** — single binary: `Cli` (clap `Parser`), subcommand `build`, default search mode when no subcommand.
- **`tests/cli_smoke.rs`** — spawns the `sift` binary for end-to-end checks.

## Behavior notes

- Global options (e.g. `--index`) must appear **before** `build` when indexing.
- Search paths are resolved and must sit under the corpus root recorded in the index metadata (see main error messages in `main.rs`).
- Prefer extending flags by threading new `SearchMatchFlags` / `SearchOptions` fields through to `CompiledSearch::new` in core rather than duplicating regex logic here.

## Commands

```bash
cargo test -p sift-cli
cargo build --release -p sift-cli
```
