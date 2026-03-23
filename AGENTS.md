# Agent notes (sift workspace)

## Layout

| Path | Role |
|------|------|
| `crates/core` | `sift-core` — index build, `Index`, `CompiledSearch`, search pipeline |
| `crates/cli` | `sift-cli` — `sift` binary (clap), thin wrapper over core |
| `fuzz/` | `cargo-fuzz` crate (excluded from workspace); see `fuzz/README.md` |
| `scripts/` | `bench.sh`, `profile.sh`, `fuzz.sh`, smoke helpers |
| `skills/` | Installable agent skills for [skills.sh](https://skills.sh) / `npx skills` (see `skills/README.md`) |
| `BENCH.md` | Criterion + profiling workflow |
| `plan.md` | Product / design roadmap (human-oriented) |

## Commands

```bash
cargo fmt --all -- --check
cargo clippy-check   # alias: clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
./scripts/bench.sh
```

**CI:** `.github/workflows/ci.yml` runs the same `fmt` / `clippy` / `test` steps on pushes and PRs to `main` / `master` on **Ubuntu, macOS, and Windows** (stable Rust, `Swatinem/rust-cache`, `fail-fast: false`). Fuzz stays manual (`./scripts/fuzz.sh`).

`cargo bench` / `sift-profile` need the right package and features; see `BENCH.md` and `crates/core/README.md`.

## Conventions

- Workspace lints: `unsafe` forbidden; clippy pedantic/nursery/cargo as warn (treat `-D warnings` in CI as hard).
- Prefer small, focused changes; match existing style.
- Do not commit `target/`, `.cursor/`, local `.index/` (see root `.gitignore`).

## Embedding / API

Consumers typically call `build_index`, `Index::open`, `CompiledSearch::new`, then `search_index` or `search_walk`. Details live in `crates/core/README.md`.
