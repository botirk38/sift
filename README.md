# sift

Rust workspace: **`sift-core`** (search engine) and **`sift`** (thin CLI). See `plan.md` for the roadmap. Benchmarks and profiling: [`BENCH.md`](BENCH.md).

## CLI (ripgrep-shaped)

- **Search:** `sift [OPTIONS] PATTERN [PATH...]` — optional paths limit hits to files under those roots (relative to the current directory); each path must lie under the indexed corpus root.
- **Index:** `sift --index <dir> build [corpus]` — build or refresh the index (put global options like `--index` before the `build` subcommand).
- **Patterns:** Rust `regex` syntax unless `-F` (fixed string). For a literal `build`, use `-e build` or `sift -- build`.
- **Differences from ripgrep:** search requires an existing index (`build` first); no `-g` / smart-case / parallel search in the CLI yet; `--no-filename` instead of `-h` (reserved for help).
