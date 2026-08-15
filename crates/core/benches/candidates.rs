//! File planning and resolution benchmarks.
//!
//! Exercises candidate resolution through the public `Plan::resolve` API.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::Path;

use sift_core::{
    CorpusMeta, FileFilter, FileFilterConfig, FileOrder, FilterMeta, GramWidth, IndexCoverage,
    IndexRecord, Indexes, Plan, Query, Scan, ScanScope, SearchMode, SearchOptions, Searcher,
    SnapshotFreshness, StoreMeta, VisibilityConfig, WalkMeta, ZeroCounts,
};

mod common;

struct PlannerFixture {
    _temp: tempfile::TempDir,
    indexes: Indexes,
    filter: FileFilter,
    complete_meta: StoreMeta,
    lazy_meta: StoreMeta,
}

fn sift_criterion() -> Criterion {
    Criterion::default()
        .warm_up_time(std::time::Duration::from_secs(3))
        .measurement_time(std::time::Duration::from_secs(6))
        .sample_size(100)
        .significance_level(0.05)
        .noise_threshold(0.05)
        .configure_from_args()
}

fn store_meta(root: &Path, coverage: IndexCoverage) -> StoreMeta {
    StoreMeta::new(
        CorpusMeta {
            root: root.to_path_buf(),
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        },
        coverage,
        WalkMeta {
            follow_links: false,
            one_file_system: false,
            max_depth: None,
            max_filesize: None,
        },
        FilterMeta {
            visibility: VisibilityConfig::default(),
        },
        vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
    )
}

fn planner_fixture() -> PlannerFixture {
    let (temp, indexes) = common::open_large_indexes();
    let root = indexes.corpus_root().to_path_buf();
    let filter = FileFilter::new(&FileFilterConfig::default(), &root).unwrap();
    PlannerFixture {
        _temp: temp,
        indexes,
        filter,
        complete_meta: store_meta(&root, IndexCoverage::Complete),
        lazy_meta: store_meta(&root, IndexCoverage::Lazy),
    }
}

fn empty_index_fixture() -> (tempfile::TempDir, Indexes, FileFilter) {
    let temp = tempfile::tempdir().unwrap();
    let corpus = temp.path().join("corpus");
    common::make_filter_corpus(&corpus);
    let sift_dir = temp.path().join(".sift");
    let meta = store_meta(&corpus, IndexCoverage::Complete);
    let indexes = Indexes::open(&sift_dir, &meta).unwrap();
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).unwrap();
    (temp, indexes, filter)
}

fn resolve(
    fixture: &PlannerFixture,
    patterns: &[String],
    options: SearchOptions,
    scope: ScanScope,
    mode: SearchMode,
    meta: Option<&StoreMeta>,
) -> usize {
    let source = Scan::new(Some(&fixture.indexes), &fixture.filter, meta, scope);
    let query = Query::new(patterns.to_vec(), options).unwrap();
    let searcher = Searcher::new(query).unwrap();
    Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .unwrap()
        .into_vec()
        .len()
}

fn bench_candidate_planner(c: &mut Criterion) {
    let fixture = planner_fixture();
    let literal = vec!["[A-Z]+_RESUME".to_string()];
    let no_literal = vec![r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}".to_string()];
    let index_scope = |freshness: SnapshotFreshness| ScanScope::Index {
        order: FileOrder::default(),
        freshness,
    };

    let mut g = c.benchmark_group("candidate_planner");

    g.bench_function("use_index_literal", |b| {
        b.iter(|| {
            black_box(resolve(
                &fixture,
                &literal,
                SearchOptions::default(),
                index_scope(SnapshotFreshness::Current),
                SearchMode::Lines,
                Some(&fixture.complete_meta),
            ));
        });
    });

    g.bench_function("all_indexed_complete", |b| {
        b.iter(|| {
            black_box(resolve(
                &fixture,
                &no_literal,
                SearchOptions::default(),
                index_scope(SnapshotFreshness::Current),
                SearchMode::CountLines {
                    zeros: ZeroCounts::Include,
                },
                Some(&fixture.complete_meta),
            ));
        });
    });

    g.bench_function("lazy_merge_index_and_walk", |b| {
        b.iter(|| {
            black_box(resolve(
                &fixture,
                &literal,
                SearchOptions::default(),
                index_scope(SnapshotFreshness::Current),
                SearchMode::Lines,
                Some(&fixture.lazy_meta),
            ));
        });
    });

    g.finish();
}

fn bench_candidate_planner_walk(c: &mut Criterion) {
    let (_temp, indexes, filter) = empty_index_fixture();
    let patterns = vec!["beta".to_string()];
    let query = Query::new(patterns, SearchOptions::default()).unwrap();
    let searcher = Searcher::new(query).unwrap();
    let mode = SearchMode::Lines;
    let scope = ScanScope::Index {
        order: FileOrder::default(),
        freshness: SnapshotFreshness::Current,
    };

    let mut g = c.benchmark_group("candidate_planner");
    g.bench_function("walk_fallback_empty_index", |b| {
        b.iter(|| {
            let source = Scan::new(Some(&indexes), &filter, None, scope);
            black_box(
                Plan::new(&source, searcher.query(), mode.coverage())
                    .resolve(&source)
                    .unwrap()
                    .into_vec()
                    .len(),
            );
        });
    });
    g.finish();
}

criterion_group! {
    name = benches;
    config = sift_criterion();
    targets = bench_candidate_planner, bench_candidate_planner_walk
}
criterion_main!(benches);
