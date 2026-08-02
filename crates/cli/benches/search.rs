use criterion::Criterion;
use std::hint::black_box;

use sift_core::Searcher;

use crate::support::{args, build_index, make_small_corpus, parse_cli, run_sift};

fn bench_binary_mode(g: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    let cli_default = parse_cli(&["pattern"]);
    let cli_text = parse_cli(&["-a", "pattern"]);
    let argv_storage_default = args(&["sift", "pattern"]);
    let argv_storage_text = args(&["sift", "-a", "pattern"]);
    let argv_default = sift_grep::Argv::new(&argv_storage_default);
    let argv_text = sift_grep::Argv::new(&argv_storage_text);
    let pat_default =
        sift_grep::pattern::PatternArgv::resolve(&argv_default, sift_core::ZeroCounts::Omit);
    let pat_text =
        sift_grep::pattern::PatternArgv::resolve(&argv_text, sift_core::ZeroCounts::Omit);

    g.bench_function("binary_mode/default", |b| {
        let config = cli_default.pattern_config();
        b.iter(|| {
            black_box(
                config
                    .query(vec!["pattern".to_string()], black_box(&pat_default))
                    .and_then(Searcher::new)
                    .unwrap()
                    .options()
                    .binary_mode,
            )
        });
    });
    g.bench_function("binary_mode/text", |b| {
        let config = cli_text.pattern_config();
        b.iter(|| {
            black_box(
                config
                    .query(vec!["pattern".to_string()], black_box(&pat_text))
                    .and_then(Searcher::new)
                    .unwrap()
                    .options()
                    .binary_mode,
            )
        });
    });
}

pub fn bench(c: &mut Criterion) {
    let mut g = c.benchmark_group("search_dispatch");

    // run_type_list
    g.bench_function("type_list", |b| {
        let tmp = tempfile::tempdir().unwrap();
        b.iter(|| run_sift(&["--type-list"], tmp.path()));
    });

    bench_binary_mode(&mut g);

    // run_files_mode — subprocess
    let tmp_files = tempfile::tempdir().unwrap();
    let f_corpus = tmp_files.path().join("corpus");
    make_small_corpus(&f_corpus, 20);
    let no_index = tmp_files.path().join(".sift-absent");

    g.bench_function("files_mode/default", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &no_index.to_string_lossy(), "--files"],
                &f_corpus,
            );
        });
    });

    // Walk search dispatch
    g.bench_function("walk_literal", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &no_index.to_string_lossy(), "beta"],
                &f_corpus,
            );
        });
    });
    g.bench_function("walk_no_literal", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &no_index.to_string_lossy(), r"\w{10}"],
                &f_corpus,
            );
        });
    });
    g.bench_function("walk_json", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &no_index.to_string_lossy(), "--json", "beta"],
                &f_corpus,
            );
        });
    });
    g.bench_function("walk_count", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &no_index.to_string_lossy(), "-c", "beta"],
                &f_corpus,
            );
        });
    });

    // Indexed search dispatch
    let tmp_idx = tempfile::tempdir().unwrap();
    let i_corpus = tmp_idx.path().join("corpus");
    make_small_corpus(&i_corpus, 20);
    let sift_dir = tmp_idx.path().join(".sift");
    build_index(&i_corpus, &sift_dir);

    g.bench_function("indexed_literal", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &sift_dir.to_string_lossy(), "beta"],
                &i_corpus,
            );
        });
    });
    g.bench_function("indexed_no_literal", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &sift_dir.to_string_lossy(), r"\w{10}"],
                &i_corpus,
            );
        });
    });
    g.bench_function("indexed_json", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &sift_dir.to_string_lossy(), "--json", "beta"],
                &i_corpus,
            );
        });
    });
    g.bench_function("indexed_count", |b| {
        b.iter(|| {
            run_sift(
                &["--sift-dir", &sift_dir.to_string_lossy(), "-c", "beta"],
                &i_corpus,
            );
        });
    });

    g.finish();
}
