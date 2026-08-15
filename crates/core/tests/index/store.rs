use std::fs;

use sift_core::search::SearchOptions;
use sift_core::{FilterAdmission, GramWidth, IndexRecord, Indexes};
use tempfile::TempDir;

use super::common::{build_indexes, index_candidates, open_indexes, sample_store_meta};

#[test]
fn build_and_reopen_indexes() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("a.txt"), "hello world\n").expect("write");

    let sift_dir = tmp.path().join(".sift");
    build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    assert!(indexes.queryable());
    let files = index_candidates(
        &indexes,
        &corpus,
        &["hello".to_string()],
        SearchOptions::default(),
        FilterAdmission::Full,
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].rel_path().as_os_str(), "a.txt");
}

#[test]
fn build_always_publishes_new_snapshot() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("f.txt"), "hello\n").expect("write");

    let sift_dir = tmp.path().join(".sift");
    let corpus_path = corpus.clone();
    let root = corpus.canonicalize().unwrap_or(corpus_path);
    let meta = sample_store_meta(root, vec![IndexRecord::ngram(GramWidth::TRIGRAM)]);
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build");
    let id = indexes.current_id().expect("id").to_string();

    indexes.build().expect("rebuild");
    assert_ne!(
        indexes.current_id().unwrap(),
        id,
        "full rebuild publishes a new snapshot id"
    );
}
