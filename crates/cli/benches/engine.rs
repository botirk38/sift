// Benchmarks for src/engine.rs — EngineDecl, ThreadingDecl, WalkerDecl, MultilineDecl.
//
// These structs carry no runtime logic (they are pure clap Args), so benchmarks
// measure clap parse cost for each flag group. Broader flag-combination coverage
// lives in the cli bench group.

use criterion::Criterion;
use std::hint::black_box;

use crate::support::parse_cli;

pub fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("engine");

    g.bench_function("engine_flags", |b| {
        b.iter(|| {
            black_box(parse_cli(&[
                "--no-config",
                "--unicode",
                "--colors",
                "path:fg:red",
                "--regex-size-limit",
                "10M",
                "--dfa-size-limit",
                "50M",
                "pattern",
            ]))
        });
    });

    g.bench_function("threading_flags", |b| {
        b.iter(|| {
            black_box(parse_cli(&[
                "-j",
                "8",
                "--line-buffered",
                "--path-separator",
                "/",
                "pattern",
            ]))
        });
    });

    g.bench_function("walker_flags", |b| {
        b.iter(|| black_box(parse_cli(&["--one-file-system", "--io", "mmap", "pattern"])));
    });

    g.bench_function("multiline_flags", |b| {
        b.iter(|| {
            black_box(parse_cli(&[
                "-U",
                "--multiline-dotall",
                "--crlf",
                "pattern",
            ]))
        });
    });

    g.bench_function("combined_engine", |b| {
        b.iter(|| {
            black_box(parse_cli(&[
                "--no-config",
                "--unicode",
                "-j",
                "8",
                "--one-file-system",
                "-U",
                "--crlf",
                "pattern",
            ]))
        });
    });

    g.finish();
}
