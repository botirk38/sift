use std::fs;

use sift_core::grep::FilterAdmission;
use sift_core::search::SearchOptions;
use sift_core::{GramWidth, Index, IndexRecord, Indexes, NGramIndex};
use tempfile::TempDir;

use super::common::{
    build_indexes, index_candidates, open_indexes, sample_store_meta, standard_build_config,
};

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
fn update_skips_rebuild_when_unchanged() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    fs::write(corpus.join("f.txt"), "hello\n").expect("write");

    let sift_dir = tmp.path().join(".sift");
    let config = standard_build_config(&corpus, &[]);
    let corpus_path = corpus.clone();
    let root = corpus.canonicalize().unwrap_or(corpus_path);
    let meta = sample_store_meta(root, vec![IndexRecord::ngram(GramWidth::TRIGRAM)]);
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    let catalog: Vec<Box<dyn Index>> = vec![Box::new(NGramIndex::new().width(GramWidth::TRIGRAM))];
    indexes.build(&catalog, &config, &[]).expect("build");
    let id = indexes.current_id().expect("id").to_string();

    let changed = indexes.update(&catalog, &[]).expect("update");
    assert_eq!(changed, None, "expected no rebuild when corpus unchanged");
    assert_eq!(indexes.current_id().unwrap(), id);
}
