use std::fs;
use std::path::Path;

use sift_core::{
    CorpusMeta, FileId, Files, FilterMeta, GramNorm, GramWidth, IndexCoverage, IndexRecord,
    Indexes, NGramIndex, StoreMeta, VisibilityConfig, WalkMeta,
};
use tempfile::TempDir;

fn default_meta() -> StoreMeta {
    StoreMeta::new(
        CorpusMeta {
            root: std::path::PathBuf::new(),
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
        IndexRecord::default_catalog(),
    )
}

#[test]
fn open_missing_current_returns_empty_registry() {
    let tmp = TempDir::new().expect("tempdir");
    let meta = default_meta();
    let indexes = Indexes::open(tmp.path(), &meta).expect("open");
    assert!(!indexes.queryable());
}

#[test]
fn open_empty_sift_dir_returns_empty_registry() {
    let tmp = TempDir::new().expect("tempdir");
    let sift_dir = tmp.path().join(".sift");
    fs::create_dir_all(&sift_dir).expect("mkdir");
    let meta = default_meta();
    let indexes = Indexes::open(&sift_dir, &meta).expect("open");
    assert!(!indexes.queryable());
}

#[test]
fn open_broken_current_errors() {
    let tmp = TempDir::new().expect("tempdir");
    let sift_dir = tmp.path().join(".sift");
    fs::create_dir_all(&sift_dir).expect("mkdir");
    fs::write(sift_dir.join("CURRENT"), "nonexistent-snapshot-id\n").expect("write");
    let meta = default_meta();
    assert!(Indexes::open(&sift_dir, &meta).is_err());
}

#[test]
fn scoped_directory_indexes_correctly() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    let file = corpus.join("one.txt");
    fs::write(&file, "alpha\nbeta needle\n").expect("write");

    let root = corpus.canonicalize().expect("canonicalize");
    let meta = StoreMeta::new(
        CorpusMeta {
            root,
            include_paths: vec![Path::new("one.txt").to_path_buf()],
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
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir(&snapshot).expect("snapshot");
    let files = Files::build(&meta, &snapshot).expect("files");
    let trigram_dir = tmp.path().join("trigram");
    NGramIndex::build(GramWidth::TRIGRAM, GramNorm::Identity, &trigram_dir, &files).expect("build");
    let index = NGramIndex::open(
        GramWidth::TRIGRAM,
        GramNorm::Identity,
        &trigram_dir,
        files.len(),
    )
    .expect("open");

    assert_eq!(index.file_count(), 1);
    assert_eq!(files.rel_path(FileId::new(0)), Some("one.txt"));
    assert!(files.rel_path(FileId::new(1)).is_none());
}
