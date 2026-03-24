# sift

**Indexed** regex search over a codebase: build a trigram index once, then query it with a grep-like CLI or the **`sift-core`** library.

| Crate | Package | Purpose |
|-------|---------|---------|
| `crates/core` | `sift-core` | Index + `CompiledSearch` + `search_index` / `search_walk` |
| `crates/cli` | `sift-cli` | `sift` binary (ripgrep-shaped flags) |
| `fuzz/` | (standalone) | `cargo-fuzz` against `sift-core` only |

**Docs:** [`crates/core/benches/README.md`](crates/core/benches/README.md) (benchmarks & profiling), [`plan.md`](plan.md) (roadmap), [`AGENTS.md`](AGENTS.md) (repo / automation hints). Per-crate **`README.md`** and **`AGENTS.md`** live under each crate and under `fuzz/`.

**Agent skills** ([skills.sh](https://skills.sh) / `npx skills`): [`skills/README.md`](skills/README.md).

## Quick start

```bash
cargo build --release -p sift-cli
./target/release/sift --sift-dir .sift build /path/to/corpus
./target/release/sift --sift-dir .sift pattern
```

Patterns use Rust’s **`regex`** syntax unless **`-F`** (fixed string). Literal **`build`**: `sift -- build` or `-e build`.

## CLI vs ripgrep (short)

- Search needs a **prior index** (`build`).
- Optional path arguments must lie **under** the indexed corpus root.
- No glob `-g` here yet; **`--no-filename`** is used instead of **`-h`** (help).

## Performance snapshot

Current Linux benchsuite snapshot against the Linux corpus.

- correctness parity: **11/11**
- `sift` faster: **8/11**
- `rg` faster: **3/11**

![Linux performance summary](docs/perf/linux-summary.jpg)

| Search class | Snapshot | Takeaway |
|---|---:|---|
| Indexed literals | `~5.8x` faster | Trigram narrowing is doing the heavy lifting |
| Indexed word matches | `~5.6x` faster | Whole-word literal shaping stays cheap |
| Indexed alternation | `~2.6x` faster | Candidate narrowing plus `build_many` helps a lot |
| Full-scan Unicode | `~1.0x` | Near parity overall, but Greek classes still trail |
| Full-scan no-literal regex | `~0.6x` | Regex-engine full scans remain the hardest cases |

Fast path takeaways:

- indexed literal, word, suffix-literal, and alternation searches are decisively faster with `sift`
- full-scan Unicode class searches are the main remaining gap versus `rg`
- see [`crates/core/benches/README.md`](crates/core/benches/README.md) for the benchmark and profiling workflow

## Develop

```bash
cargo test --workspace --all-features
cargo clippy-check   # see `.cargo/config.toml`
```

CI (GitHub Actions): **`fmt`**, **`clippy`** with **`-D warnings`**, **`test`** on **Linux, macOS, and Windows** for pushes/PRs to `main` / `master` — [`.github/workflows/ci.yml`](.github/workflows/ci.yml).
