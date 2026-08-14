//! Index build, open, candidate, and persistence benchmarks.
//!
//! Exercises public N-gram index, `Indexes`, and `Index` APIs.
//! Storage effects are measured indirectly through build/open/save/reopen paths.

use criterion::{Criterion, criterion_group, criterion_main};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};

use sift_core::{
    CaseMode, CorpusKind, CorpusMeta, FileFilter, FileFilterConfig, FileOrder, Files, FilterMeta,
    GramNorm, GramWidth, IndexCoverage, IndexRecord, Indexes, NGramIndex, Plan, Query, Scan,
    ScanScope, SearchMode, SearchOptions, Searcher, SnapshotFreshness, StoreMeta, VisibilityConfig,
    WalkMeta,
};

mod common;

fn build_files(corpus: &Path) -> Files {
    let (root, kind, include_paths) = if corpus.is_file() {
        let parent = corpus.parent().unwrap_or(corpus);
        let filename = corpus.file_name().map(PathBuf::from).unwrap_or_default();
        (parent.to_path_buf(), CorpusKind::SingleFile, vec![filename])
    } else {
        (corpus.to_path_buf(), CorpusKind::Directory, vec![])
    };
    let abs_root = root.canonicalize().unwrap_or(root);
    let meta = StoreMeta::new(
        CorpusMeta {
            root: abs_root,
            kind,
            include_paths,
            exclude_paths: Vec::new(),
        },
        IndexCoverage::Complete,
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
    );
    Files::build(&meta).unwrap()
}

fn build_index(corpus: &Path, idx_dir: &Path) -> NGramIndex {
    let files = build_files(corpus);
    NGramIndex::build(GramWidth::TRIGRAM, GramNorm::Identity, idx_dir, &files).unwrap();
    NGramIndex::open(GramWidth::TRIGRAM, GramNorm::Identity, idx_dir, files.len()).unwrap()
}

fn open_index(idx_dir: &Path, file_count: usize) -> NGramIndex {
    NGramIndex::open(GramWidth::TRIGRAM, GramNorm::Identity, idx_dir, file_count).unwrap()
}

fn index_candidate_vec(
    indexes: &Indexes,
    filter: &FileFilter,
    patterns: &[String],
    options: SearchOptions,
) -> Vec<sift_core::File> {
    let source = Scan::new(
        Some(indexes),
        filter,
        None,
        ScanScope::Index {
            order: FileOrder::default(),
            freshness: SnapshotFreshness::Current,
        },
    );
    let query = Query::new(patterns.to_vec(), options).unwrap();
    let searcher = Searcher::new(query).unwrap();
    Plan::new(&source, searcher.query(), SearchMode::Lines.coverage())
        .resolve(&source)
        .unwrap()
        .into_vec()
}

struct IndexOpenFixture {
    temp: tempfile::TempDir,
    idx_dir: std::path::PathBuf,
    file_count: usize,
}

struct SiftDirFixture {
    temp: tempfile::TempDir,
    sift_dir: std::path::PathBuf,
    meta: StoreMeta,
}

fn default_meta(root: std::path::PathBuf) -> StoreMeta {
    StoreMeta::new(
        CorpusMeta {
            root,
            kind: CorpusKind::Directory,
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        },
        sift_core::IndexCoverage::Complete,
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

impl Drop for IndexOpenFixture {
    fn drop(&mut self) {
        let _ = &mut self.temp;
    }
}

impl Drop for SiftDirFixture {
    fn drop(&mut self) {
        let _ = &mut self.temp;
    }
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

// ─── Index-only corpus helpers ───────────────────────────────────────────────

fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha beta\n").unwrap();
    fs::write(root.join("b/y.txt"), "gamma delta\n").unwrap();
}

fn make_single_file_corpus(root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("single.rs"),
        "fn main() {\n    let x = 42;\n    println!(\"beta: {}\", x);\n}\n",
    )
    .unwrap();
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

fn materialize_monorepo_corpus(
    root: &Path,
    files: usize,
    lines_per_file: usize,
    dir_fanout: usize,
) {
    common::materialize_large_corpus(root, files, lines_per_file, dir_fanout);
}

// ─── Index-only build helpers ────────────────────────────────────────────────

/// Full `sift build` path via [`Indexes`] (production defaults).
fn build_index_via_store(corpus: &Path, sift_dir: &Path) {
    let corpus_path = corpus.to_path_buf();
    let root = corpus.canonicalize().unwrap_or(corpus_path);
    let meta = StoreMeta::new(
        CorpusMeta {
            root,
            kind: CorpusKind::Directory,
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        },
        IndexCoverage::Complete,
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
    );
    let mut indexes = Indexes::open(sift_dir, &meta).unwrap();
    indexes.refresh_meta(&meta).unwrap();
    indexes.build().unwrap();
}

// ─── Build benchmarks ────────────────────────────────────────────────────────

fn bench_index_build(c: &mut Criterion) {
    let mut g = c.benchmark_group("index_build");

    g.bench_function("single_file", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_single_file_corpus(&corpus);
            let idx = tmp.path().join(".sift");
            build_index_via_store(&corpus, &idx);
        });
    });

    g.bench_function("small_corpus", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_parity_corpus(&corpus);
            let idx = tmp.path().join(".sift");
            build_index_via_store(&corpus, &idx);
        });
    });

    g.bench_function("filter_corpus", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            common::make_filter_corpus(&corpus);
            let idx = tmp.path().join(".sift");
            build_index_via_store(&corpus, &idx);
        });
    });

    g.bench_function("many_tiny_files", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_many_files_corpus(&corpus, 1_000);
            let idx = tmp.path().join(".sift");
            build_index_via_store(&corpus, &idx);
        });
    });

    g.bench_function("monorepo", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            materialize_monorepo_corpus(&corpus, 8_000, 100, 256);
            let idx = tmp.path().join(".sift");
            build_index_via_store(&corpus, &idx);
        });
    });

    // Corpus materialized once, so each iteration measures only index build
    // (walk, gram extraction, posting assembly) without filesystem write cost.
    g.bench_function("prebuilt_monorepo", |b| {
        let tmp = tempfile::tempdir().unwrap();
        let corpus = tmp.path().join("corpus");
        materialize_monorepo_corpus(&corpus, 8_000, 100, 256);
        b.iter(|| {
            let idx = tempfile::tempdir().unwrap();
            build_index_via_store(&corpus, idx.path());
        });
    });

    g.finish();
}

// ─── Rebuild benchmarks ──────────────────────────────────────────────────────

struct RebuildFixture {
    _temp: tempfile::TempDir,
    corpus: PathBuf,
    out_dir: PathBuf,
}

fn build_rebuild_fixture(files: usize, lines_per_file: usize, dir_fanout: usize) -> RebuildFixture {
    let temp = tempfile::tempdir().unwrap();
    let corpus = temp.path().join("corpus");
    common::materialize_large_corpus(&corpus, files, lines_per_file, dir_fanout);
    let out_dir = temp.path().join(".sift-rebuild");
    RebuildFixture {
        _temp: temp,
        corpus,
        out_dir,
    }
}

fn bench_index_rebuild(c: &mut Criterion) {
    const FILES: usize = 2_000;
    const LINES: usize = 60;
    const FANOUT: usize = 64;

    let mut g = c.benchmark_group("index_rebuild");

    g.bench_function("full_rebuild", |b| {
        let fx = build_rebuild_fixture(FILES, LINES, FANOUT);
        b.iter(|| {
            let files = build_files(&fx.corpus);
            NGramIndex::build(GramWidth::TRIGRAM, GramNorm::Identity, &fx.out_dir, &files).unwrap();
            black_box(files.len());
        });
    });

    g.finish();
}

fn bench_index_open(c: &mut Criterion) {
    let mut g = c.benchmark_group("index_open");

    g.bench_function("small", |b| {
        let fixture = {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_parity_corpus(&corpus);
            let idx = tmp.path().join(".sift");
            let files = build_files(&corpus);
            let file_count = files.len();
            build_index(&corpus, &idx);
            IndexOpenFixture {
                temp: tmp,
                idx_dir: idx,
                file_count,
            }
        };
        b.iter(|| {
            black_box(open_index(&fixture.idx_dir, fixture.file_count));
        });
    });

    g.bench_function("large", |b| {
        let fixture = {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            common::materialize_large_corpus(&corpus, 8_000, 100, 256);
            let idx = tmp.path().join(".sift");
            let files = build_files(&corpus);
            let file_count = files.len();
            build_index(&corpus, &idx);
            IndexOpenFixture {
                temp: tmp,
                idx_dir: idx,
                file_count,
            }
        };
        b.iter(|| {
            black_box(open_index(&fixture.idx_dir, fixture.file_count));
        });
    });

    g.finish();
}

// ─── Indexes::open benchmarks ────────────────────────────────────────────────

fn bench_indexes_open(c: &mut Criterion) {
    let mut g = c.benchmark_group("indexes_open");

    g.bench_function("empty_registry", |b| {
        b.iter(|| {
            let tmp = tempfile::tempdir().unwrap();
            let sift_dir = tmp.path().join(".sift");
            std::fs::create_dir_all(&sift_dir).unwrap();
            let meta = default_meta(std::path::PathBuf::new());
            black_box(Indexes::open(&sift_dir, &meta).unwrap());
        });
    });

    g.bench_function("one_trigram_index", |b| {
        let fixture = {
            let tmp = tempfile::tempdir().unwrap();
            let corpus = tmp.path().join("corpus");
            make_parity_corpus(&corpus);
            let sift = tmp.path().join(".sift");
            let corpus_path = corpus.clone();
            let root = corpus.canonicalize().unwrap_or(corpus_path);
            let meta = StoreMeta::new(
                CorpusMeta {
                    root,
                    kind: CorpusKind::Directory,
                    include_paths: Vec::new(),
                    exclude_paths: Vec::new(),
                },
                sift_core::IndexCoverage::Complete,
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
            );
            let mut indexes = Indexes::open(&sift, &meta).expect("open indexes");
            indexes.refresh_meta(&meta).expect("refresh meta");
            indexes.build().expect("build");
            drop(indexes);
            SiftDirFixture {
                temp: tmp,
                sift_dir: sift,
                meta,
            }
        };
        b.iter(|| {
            black_box(Indexes::open(&fixture.sift_dir, &fixture.meta).unwrap());
        });
    });

    g.finish();
}

// ─── Save/reopen benchmarks ──────────────────────────────────────────────────

fn bench_index_save_reopen(c: &mut Criterion) {
    let mut g = c.benchmark_group("index_save_reopen");

    g.bench_function("reopen", |b| {
        let tmp = tempfile::tempdir().unwrap();
        let corpus = tmp.path().join("corpus");
        make_parity_corpus(&corpus);
        let idx_dir = tmp.path().join(".sift");
        let index = build_index(&corpus, &idx_dir);
        let file_count = index.file_count();
        drop(index);
        b.iter(|| {
            black_box(open_index(&idx_dir, file_count));
        });
    });

    g.finish();
}

// ─── File benches ───────────────────────────────────────────────────────

fn bench_candidates(c: &mut Criterion) {
    let fixture = common::open_large_indexes();
    let indexes = fixture.1;
    let root = indexes.corpus_root().to_path_buf();
    let filter = FileFilter::new(&FileFilterConfig::default(), &root).unwrap();

    let mut g = c.benchmark_group("index_candidates");

    g.bench_function("literal", |b| {
        let patterns = ["beta".to_string()];
        b.iter(|| {
            black_box(index_candidate_vec(
                &indexes,
                &filter,
                &patterns,
                SearchOptions::default(),
            ))
        });
    });

    g.bench_function("required_literal", |b| {
        let patterns = ["[A-Z]+_RESUME".to_string()];
        b.iter(|| {
            black_box(index_candidate_vec(
                &indexes,
                &filter,
                &patterns,
                SearchOptions::default(),
            ))
        });
    });

    g.bench_function("full_scan_fallback", |b| {
        let patterns = [r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}".to_string()];
        b.iter(|| {
            black_box(index_candidate_vec(
                &indexes,
                &filter,
                &patterns,
                SearchOptions::default(),
            ))
        });
    });

    g.bench_function("alternation", |b| {
        let patterns = ["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
        b.iter(|| {
            black_box(index_candidate_vec(
                &indexes,
                &filter,
                &patterns,
                SearchOptions::default(),
            ))
        });
    });

    g.bench_function("case_insensitive", |b| {
        let patterns = ["beta".to_string()];
        let options = SearchOptions {
            case_mode: CaseMode::Insensitive,
            ..SearchOptions::default()
        };
        b.iter(|| {
            black_box(index_candidate_vec(
                &indexes,
                &filter,
                &patterns,
                options.clone(),
            ))
        });
    });

    g.bench_function("case_insensitive_alternation", |b| {
        let patterns = ["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()];
        let options = SearchOptions {
            case_mode: CaseMode::Insensitive,
            ..SearchOptions::default()
        };
        b.iter(|| {
            black_box(index_candidate_vec(
                &indexes,
                &filter,
                &patterns,
                options.clone(),
            ))
        });
    });

    g.finish();
}

criterion_group! {
    name = benches;
    config = sift_criterion();
    targets = bench_index_build, bench_index_rebuild, bench_index_open, bench_indexes_open, bench_index_save_reopen, bench_candidates,
}
criterion_main!(benches);
