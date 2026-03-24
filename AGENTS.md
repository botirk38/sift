# Agent notes (sift workspace)

## Layout

| Path | Role |
|------|------|
| `crates/core` | `sift-core` — index build, `Index`, `CompiledSearch`, search pipeline |
| `crates/cli` | `sift-cli` — `sift` binary (clap), thin wrapper over core |
| `fuzz/` | `cargo-fuzz` crate (excluded from workspace); see `fuzz/README.md` |
| `scripts/` | `bench.sh`, `profile.sh`, `fuzz.sh`, integration helpers |
| `skills/` | Installable agent skills for [skills.sh](https://skills.sh) / `npx skills` (see `skills/README.md`) |
| `crates/core/benches/README.md` | Criterion + profiling workflow |
| `plan.md` | Product / design roadmap (human-oriented) |

## Commands

```bash
cargo fmt --all -- --check
cargo clippy-check   # alias: clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
./scripts/bench.sh
```

**CI:** `.github/workflows/ci.yml` runs the same `fmt` / `clippy` / `test` steps on pushes and PRs to `main` / `master` on **Ubuntu, macOS, and Windows** (stable Rust, `Swatinem/rust-cache`, `fail-fast: false`). Fuzz stays manual (`./scripts/fuzz.sh`).

`cargo bench` / `sift-profile` need the right package and features; see `crates/core/benches/README.md` and `crates/core/README.md`.

## Conventions

- Workspace lints: `unsafe` forbidden; clippy pedantic/nursery/cargo as warn (treat `-D warnings` in CI as hard).
- Prefer small, focused changes; match existing style.
- Do not commit `target/`, `.cursor/`, local `.sift/` (see root `.gitignore`).

## Embedding / API

Consumers typically call `IndexBuilder::build`, `Index::open`, `CompiledSearch::new`, then `run_index` or `walk_file_paths`. Details live in `crates/core/README.md`.
