use std::fs;

use sift_core::{
    CorpusMeta, Files, FilterMeta, GramNorm, GramWidth, IndexCoverage, IndexRecord, NGramIndex,
    StoreMeta, VisibilityConfig, WalkMeta,
};
use tempfile::TempDir;

#[test]
fn persisted_index_reopens_with_same_file_count() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("a.txt"), "hello world\n").expect("write a");
    fs::write(corpus.join("b.txt"), "goodbye world\n").expect("write b");

    let trigram_dir = tmp.path().join("trigram");
    let root = corpus.canonicalize().expect("canonicalize");
    let meta = StoreMeta::new(
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
        vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
    );
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir(&snapshot).expect("snapshot");
    let files = Files::build(&meta, &snapshot).expect("files");
    NGramIndex::build(GramWidth::TRIGRAM, GramNorm::Identity, &trigram_dir, &files).expect("build");

    let reopened = NGramIndex::open(
        GramWidth::TRIGRAM,
        GramNorm::Identity,
        &trigram_dir,
        files.len(),
    )
    .expect("reopen");
    assert_eq!(reopened.file_count(), 2);
    assert_eq!(files.len(), 2);
    assert!(files.file(sift_core::FileId::new(0)).is_some());
    assert!(files.file(sift_core::FileId::new(1)).is_some());
    assert!(files.file(sift_core::FileId::new(2)).is_none());
}
