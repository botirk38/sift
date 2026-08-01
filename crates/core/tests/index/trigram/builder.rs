use std::fs;

use sift_core::grep::VisibilityConfig;
use sift_core::{
    CorpusKind, CorpusSpec, FileId, GramNorm, GramWidth, IndexConfig, IndexDestination,
    IndexWalkConfig, IndexWrite, NGramIndex,
};
use tempfile::TempDir;

#[test]
fn persisted_index_reopens_with_same_files() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("a.txt"), "hello world\n").expect("write a");
    fs::write(corpus.join("b.txt"), "goodbye world\n").expect("write b");

    let trigram_dir = tmp.path().join("trigram");
    let root = corpus.canonicalize().expect("canonicalize");
    let config = IndexConfig {
        corpus: CorpusSpec {
            root: &corpus,
            kind: CorpusKind::Directory,
            follow_links: false,
            include_paths: &[],
            exclude_paths: &[],
        },
        walk: IndexWalkConfig::new(false),
        visibility: VisibilityConfig::default(),
    };
    let index_config = NGramIndex::new().width(GramWidth::TRIGRAM);
    index_config
        .build(IndexWrite {
            dest: IndexDestination::Directory(&trigram_dir),
            config: &config,
            paths: &[],
        })
        .expect("build");

    let reopened = NGramIndex::open(
        GramWidth::TRIGRAM,
        GramNorm::Identity,
        &trigram_dir,
        &root,
        CorpusKind::Directory,
    )
    .expect("reopen");
    assert!(reopened.file_path(FileId::new(0)).is_some());
    assert!(reopened.file_path(FileId::new(1)).is_some());
    assert!(reopened.file_path(FileId::new(2)).is_none());
}
