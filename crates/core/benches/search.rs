//! Criterion benchmarks: index build vs narrow vs full-scan search.
//!
//! Run: `cargo bench -p sift-core` or `./scripts/bench.sh`.
//! Pass Criterion flags after `--`: `cargo bench -p sift-core -- --save-baseline main`

use std::fs;
use std::path::Path;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

use sift_core::{
    CompiledSearch, Index, IndexBuilder, SearchMatchFlags, SearchMode, SearchOptions, SearchOutput,
};

/// Same layout as `indexed_search_matches_naive_for_literal` in `lib.rs` tests.
fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha beta\n").unwrap();
    fs::write(root.join("b/y.txt"), "gamma delta\n").unwrap();
}

fn make_many_files_corpus(root: &Path, n: usize) {
    for i in 0..n {
        let dir = root.join(format!("d{}", i % 10));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("f{i}.txt")),
            format!("line one line two content {i}\n"),
        )
        .unwrap();
    }
}

/// Statistical settings: long enough measurement windows for µs-scale search ops and ~ms index builds.
fn sift_criterion() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(5))
        .measurement_time(Duration::from_secs(10))
        .sample_size(150)
        .significance_level(0.05)
        .noise_threshold(0.05)
        .configure_from_args()
}

fn bench_build_index(c: &mut Criterion) {
    let mut g = c.benchmark_group("build_index");
    g.bench_function("32_files", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_many_files_corpus(&corpus, 32);
            let idx = tmp.path().join(".sift");
            IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
        });
    });
    g.finish();
}

fn open_parity_index() -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);
    let idx = tmp.path().join(".sift");
    IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    let index = Index::open(&idx).unwrap();
    (tmp, index)
}

fn bench_search_literal_narrow(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts = SearchOptions::default();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let mut g = c.benchmark_group("search_literal_narrow");
    g.bench_function("beta_trigram_narrow", |b| {
        b.iter(|| {
            black_box(
                query
                    .run_index(
                        black_box(&index),
                        &[],
                        SearchOutput {
                            mode: SearchMode::Quiet,
                            with_filename: false,
                            line_number: false,
                        },
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

fn bench_search_full_scan(c: &mut Criterion) {
    let (_tmp, index) = open_parity_index();
    let opts_default = SearchOptions::default();
    let mut flags = SearchMatchFlags::empty();
    flags |= SearchMatchFlags::CASE_INSENSITIVE;
    let opts_ci = SearchOptions {
        flags,
        max_results: None,
    };
    let pat_dot = vec![".*".to_string()];
    let query_dot = CompiledSearch::new(&pat_dot, opts_default).unwrap();
    let pat_beta = vec!["beta".to_string()];
    let query_ci = CompiledSearch::new(&pat_beta, opts_ci).unwrap();
    let mut g = c.benchmark_group("search_full_scan");
    g.bench_function("dot_star", |b| {
        b.iter(|| {
            black_box(
                query_dot
                    .run_index(
                        black_box(&index),
                        &[],
                        SearchOutput {
                            mode: SearchMode::Quiet,
                            with_filename: false,
                            line_number: false,
                        },
                    )
                    .unwrap(),
            );
        });
    });
    g.bench_function("case_insensitive_literal", |b| {
        b.iter(|| {
            black_box(
                query_ci
                    .run_index(
                        black_box(&index),
                        &[],
                        SearchOutput {
                            mode: SearchMode::Quiet,
                            with_filename: false,
                            line_number: false,
                        },
                    )
                    .unwrap(),
            );
        });
    });
    g.finish();
}

criterion_group! {
    name = benches;
    config = sift_criterion();
    targets = bench_build_index, bench_search_literal_narrow, bench_search_full_scan
}
criterion_main!(benches);
