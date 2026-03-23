# Fuzz (`sift-core`)

LibFuzzer targets for **`sift-core`** only (not the CLI): `CompiledSearch::search_index`, `compile_search_pattern`, and related paths.

**Toolchain:** sanitizers need nightly. From the **repo root**, use the wrapper so `fuzz/rust-toolchain.toml` applies:

```bash
./scripts/fuzz.sh build search_usage
./scripts/fuzz.sh run search_usage -- -max_total_time=30
```

Or `cd fuzz && cargo fuzz run search_usage -- …`. From root without the script:  
`cargo +nightly fuzz run search_usage --manifest-path fuzz/Cargo.toml -- …`

Install once: `cargo install cargo-fuzz`.

## Targets

| Target | What it does |
|--------|----------------|
| `search_usage` | One tiny index per process (`OnceLock`); fuzzes pattern bytes + `SearchOptions`, runs `CompiledSearch::new` → `search_index`, and `compile_search_pattern`. |
| `compile_only` | Fuzzes `compile_search_pattern` only (no filesystem). |

Quick smoke: `./scripts/fuzz.sh quick` or `./scripts/fuzz.sh quick compile_only`.  
List targets: `cd fuzz && cargo fuzz list`.

## Layout

`fuzz/` is **excluded** from the root workspace (`Cargo.toml`) so it stays a normal `cargo-fuzz` package. See **`AGENTS.md`** here for agent-oriented notes.
