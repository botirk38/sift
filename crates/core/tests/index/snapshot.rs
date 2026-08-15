use std::fs;

use sift_core::{GramWidth, IndexRecord, Indexes};
use tempfile::TempDir;

use super::common::sample_store_meta;

#[test]
fn build_writes_current_snapshot() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("f.txt"), "data\n").expect("write");

    let sift_dir = tmp.path().join(".sift");
    let corpus_path = corpus.clone();
    let root = corpus.canonicalize().unwrap_or(corpus_path);
    let meta = sample_store_meta(root, vec![IndexRecord::ngram(GramWidth::TRIGRAM)]);
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build");

    let id = indexes.current_id().expect("snapshot id");
    assert!(!id.is_empty());
    let snap = indexes.snapshot_dir(id);
    assert!(snap.join("files.bin").is_file());
    assert!(snap.join("ngram-3").join("postings.bin").is_file());
    assert!(!snap.join("ngram-3").join("files.bin").exists());
}
