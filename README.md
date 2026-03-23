# sift

**Indexed** regex search over a codebase: build a trigram index once, then query it with a grep-like CLI or the **`sift-core`** library.

| Crate | Package | Purpose |
|-------|---------|---------|
| `crates/core` | `sift-core` | Index + `CompiledSearch` + `search_index` / `search_walk` |
| `crates/cli` | `sift-cli` | `sift` binary (ripgrep-shaped flags) |
| `fuzz/` | (standalone) | `cargo-fuzz` against `sift-core` only |

**Docs:** [`BENCH.md`](BENCH.md) (benchmarks & profiling), [`plan.md`](plan.md) (roadmap), [`AGENTS.md`](AGENTS.md) (repo / automation hints). Per-crate **`README.md`** and **`AGENTS.md`** live under each crate and under `fuzz/`.

## Quick start

```bash
cargo build --release -p sift-cli
./target/release/sift --index .index build /path/to/corpus
./target/release/sift --index .index pattern
```

Patterns use Rust’s **`regex`** syntax unless **`-F`** (fixed string). Literal **`build`**: `sift -- build` or `-e build`.

## CLI vs ripgrep (short)

- Search needs a **prior index** (`build`).
- Optional path arguments must lie **under** the indexed corpus root.
- No glob `-g` / smart-case here yet; **`--no-filename`** is used instead of **`-h`** (help).

## Develop

```bash
cargo test --workspace --all-features
cargo clippy-check   # see `.cargo/config.toml`
```

CI (GitHub Actions): **`fmt`**, **`clippy`** with **`-D warnings`**, **`test`** on **Linux, macOS, and Windows** for pushes/PRs to `main` / `master` — [`.github/workflows/ci.yml`](.github/workflows/ci.yml).
