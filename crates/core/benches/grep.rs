//! Grep-style search execution and filtering benchmarks.
//!
//! Exercises the public `Searcher::execute` corpus pipeline.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::Path;

use sift_core::{
    CaseMode, Events, FileFilter, FileFilterConfig, FileOrder, Indexes, Inputs, Plan, Query, Scan,
    ScanScope, SearchFlags, SearchInputs, SearchMode, SearchOptions, Searcher, SnapshotFreshness,
    StatsMode,
};

mod common;

fn sift_criterion() -> Criterion {
    Criterion::default()
        .warm_up_time(std::time::Duration::from_secs(3))
        .measurement_time(std::time::Duration::from_secs(6))
        .sample_size(100)
        .significance_level(0.05)
        .noise_threshold(0.05)
        .configure_from_args()
}

fn make_search(patterns: &[&str], opts: SearchOptions) -> (Vec<String>, SearchOptions) {
    let pats: Vec<String> = patterns.iter().map(ToString::to_string).collect();
    (pats, opts)
}

fn make_filter(config: &FileFilterConfig, root: &Path) -> FileFilter {
    FileFilter::new(config, root).unwrap()
}

fn run_grep(indexes: &Indexes, filter: &FileFilter, query: &(Vec<String>, SearchOptions)) -> bool {
    let source = Scan::new(
        Some(indexes),
        filter,
        None,
        ScanScope::Index {
            order: FileOrder::default(),
            freshness: SnapshotFreshness::Current,
        },
    );
    let query = Query::new(query.0.clone(), query.1.clone()).unwrap();
    let searcher = Searcher::new(query).unwrap();
    let mode = SearchMode::Lines;
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .unwrap();
    searcher
        .execute(
            SearchInputs {
                candidates,
                streams: Inputs::empty(),
                explicit: &[],
            },
            StatsMode::Off,
            mode,
            Events::Discard,
        )
        .unwrap()
        .found()
}

fn bench_indexed_search(c: &mut Criterion) {
    let fixture = common::open_large_indexes();
    let indexes = fixture.1;
    let root = indexes.corpus_root().to_path_buf();
    let filter = make_filter(&FileFilterConfig::default(), &root);

    let mut g = c.benchmark_group("grep_indexed");

    g.bench_function("literal", |b| {
        let query = make_search(&["beta"], SearchOptions::default());
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("required_literal", |b| {
        let query = make_search(&["[A-Z]+_RESUME"], SearchOptions::default());
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("alternation", |b| {
        let query = make_search(
            &["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT"],
            SearchOptions::default(),
        );
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("case_insensitive", |b| {
        let query = make_search(
            &["beta"],
            SearchOptions {
                case_mode: CaseMode::Insensitive,
                ..Default::default()
            },
        );
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("case_insensitive_alternation", |b| {
        let query = make_search(
            &["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT"],
            SearchOptions {
                case_mode: CaseMode::Insensitive,
                ..Default::default()
            },
        );
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("full_scan_fallback", |b| {
        let query = make_search(
            &[r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}"],
            SearchOptions::default(),
        );
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("invert_match", |b| {
        let query = make_search(
            &["beta"],
            SearchOptions {
                flags: SearchFlags::INVERT_MATCH,
                ..Default::default()
            },
        );
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.finish();
}

fn bench_walk_search(c: &mut Criterion) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    common::make_filter_corpus(&corpus);
    let filter = make_filter(&FileFilterConfig::default(), &corpus);
    let sift_dir = tmp.path().join(".sift");
    let meta = sift_core::StoreMeta::read(&sift_dir).unwrap_or_else(|_| {
        sift_core::StoreMeta::new(
            sift_core::CorpusMeta {
                root: sift_dir.clone(),
                include_paths: Vec::new(),
                exclude_paths: Vec::new(),
            },
            sift_core::IndexCoverage::Complete,
            sift_core::WalkMeta {
                follow_links: false,
                one_file_system: false,
                max_depth: None,
                max_filesize: None,
            },
            sift_core::FilterMeta {
                visibility: sift_core::VisibilityConfig::default(),
            },
            sift_core::IndexRecord::default_catalog(),
        )
    });
    let indexes = Indexes::open(&sift_dir, &meta).unwrap();

    let mut g = c.benchmark_group("grep_walk");

    g.bench_function("literal", |b| {
        let query = make_search(&["beta"], SearchOptions::default());
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.bench_function("full_scan", |b| {
        let query = make_search(&[".*"], SearchOptions::default());
        b.iter(|| black_box(run_grep(&indexes, &filter, &query)));
    });

    g.finish();
}

criterion_group! {
    name = benches;
    config = sift_criterion();
    targets = bench_indexed_search, bench_walk_search,
}
criterion_main!(benches);
