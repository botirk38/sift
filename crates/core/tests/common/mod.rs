//! Shared fixtures and helpers for sift-core integration tests.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use sift_core::{
    CorpusMeta, File, FileFilter, FileFilterConfig, FileOrder, FilterAdmission, FilterMeta,
    GramWidth, Hit, IgnoreConfig, IndexCoverage, IndexRecord, Indexes, Plan, Query, Scan,
    ScanScope, SearchOptions, SnapshotFreshness, StoreMeta, VisibilityConfig, WalkMeta,
};

pub fn sample_store_meta(root: PathBuf, indexes: Vec<IndexRecord>) -> StoreMeta {
    StoreMeta::new(
        CorpusMeta {
            root,
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
        indexes,
    )
}

pub fn sample_store_meta_no_ignore(root: PathBuf, indexes: Vec<IndexRecord>) -> StoreMeta {
    let mut meta = sample_store_meta(root, indexes);
    meta.filters.visibility = VisibilityConfig {
        ignore: IgnoreConfig::disabled(),
        ..VisibilityConfig::default()
    };
    meta
}

pub fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).expect("create dir");
    fs::create_dir_all(root.join("b")).expect("create dir");
    fs::write(root.join("a/x.txt"), "alpha beta\n").expect("write");
    fs::write(root.join("b/y.txt"), "gamma delta\n").expect("write");
}

pub fn make_filter_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).expect("create dir");
    fs::create_dir_all(root.join("a/.secret")).expect("create dir");
    fs::create_dir_all(root.join("subdir")).expect("create dir");
    fs::create_dir_all(root.join("skip")).expect("create dir");
    fs::create_dir_all(root.join("also_skip")).expect("create dir");

    fs::write(root.join("a/x.txt"), "alpha beta gamma\n").expect("write");
    fs::write(root.join("a/.hidden.txt"), "beta in hidden file\n").expect("write");
    fs::write(root.join("a/data.rs"), "fn main() {}\n").expect("write");
    fs::write(root.join("a/.secret/log"), "beta in hidden dir\n").expect("write");
    fs::write(root.join("subdir/a.txt"), "beta in subdir\n").expect("write");
    fs::write(root.join("subdir/b.log"), "no match here\n").expect("write");
    fs::write(root.join("root.txt"), "beta at root level\n").expect("write");
    fs::write(root.join("skip/ignored.txt"), "beta gitignored\n").expect("write");
    fs::write(root.join("also_skip/omit.txt"), "beta in .ignore\n").expect("write");
    fs::write(root.join("keep.txt"), "beta outside ignore rules\n").expect("write");

    fs::write(root.join(".gitignore"), "skip/**\n").expect("write gitignore");
    fs::write(root.join(".ignore"), "also_skip/**\n").expect("write ignore");
}

pub fn build_indexes(corpus: &Path, sift_dir: &Path) -> Indexes {
    let root = corpus
        .canonicalize()
        .unwrap_or_else(|_| corpus.to_path_buf());
    let meta = sample_store_meta(root, vec![IndexRecord::ngram(GramWidth::TRIGRAM)]);
    let mut indexes = Indexes::open(sift_dir, &meta).expect("open indexes");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build index");
    indexes
}

pub fn open_indexes(sift_dir: &Path) -> Indexes {
    let meta = StoreMeta::read(sift_dir).unwrap_or_else(|_| {
        sample_store_meta(PathBuf::new(), vec![IndexRecord::ngram(GramWidth::TRIGRAM)])
    });
    Indexes::open(sift_dir, &meta).expect("open indexes")
}

pub fn index_candidates(
    indexes: &Indexes,
    corpus: &Path,
    patterns: &[String],
    options: SearchOptions,
    admission: FilterAdmission,
) -> Vec<File> {
    let filter = FileFilter::new(&FileFilterConfig::default(), corpus).expect("filter");
    let root = corpus
        .canonicalize()
        .unwrap_or_else(|_| corpus.to_path_buf());
    let meta_storage = if admission == FilterAdmission::Indexed {
        Some(sample_store_meta(
            root,
            vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
        ))
    } else {
        None
    };
    let store_meta = meta_storage.as_ref();
    let source = Scan::new(
        Some(indexes),
        &filter,
        store_meta,
        ScanScope::Index {
            order: FileOrder::default(),
            freshness: SnapshotFreshness::Current,
        },
    );
    let query = Query::new(patterns.to_vec(), options).expect("query");
    let searcher = sift_core::Searcher::new(query).expect("searcher");
    Plan::new(
        &source,
        searcher.query(),
        sift_core::SearchMode::Print(Hit::Line).coverage(),
    )
    .resolve(&source)
    .expect("candidates")
    .into_vec()
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total = total.saturating_add(dir_size(&path));
        } else if let Ok(meta) = fs::metadata(&path) {
            total = total.saturating_add(meta.len());
        }
    }
    total
}
