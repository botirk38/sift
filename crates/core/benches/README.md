# Benchmarks

`crates/core/benches/search.rs` contains the Criterion benchmark for `sift-core` search performance.

Run from the repo root:

```bash
cargo bench -p sift-core --bench search
./scripts/bench.sh
./scripts/profile.sh
```

Notes:

- `./scripts/bench.sh` is the convenient default bench runner.
- `./scripts/profile.sh` drives the `sift-profile` binary behind the `profile` feature.
- Bench and profile runs need the right package / feature selection; see `crates/core/README.md` for crate-level context.
