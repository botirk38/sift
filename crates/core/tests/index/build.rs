use std::fs;
use std::path::Path;

use sift_core::search::SearchOptions;
use sift_core::{FilterAdmission, GramWidth, IndexRecord, Indexes};
use tempfile::TempDir;

use super::common::{
    build_indexes, index_candidates, make_filter_corpus, open_indexes, sample_store_meta,
    sample_store_meta_no_ignore,
};

fn candidate_paths(
    indexes: &Indexes,
    corpus: &Path,
    patterns: &[String],
    options: SearchOptions,
) -> Vec<std::path::PathBuf> {
    index_candidates(indexes, corpus, patterns, options, FilterAdmission::Indexed)
        .into_iter()
        .map(|c| c.rel_path().to_path_buf())
        .collect()
}

#[test]
fn gitignore_honored_without_git_repo() {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join(".gitignore"), "*.log\n").expect("gitignore");
    fs::write(tmp.path().join("keep.txt"), "hello\n").expect("keep");
    fs::write(tmp.path().join("skip.log"), "secret\n").expect("skip");

    let sift_dir = tmp.path().join(".sift");
    build_indexes(tmp.path(), &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let paths = candidate_paths(
        &indexes,
        tmp.path(),
        &["hello".to_string(), "secret".to_string()],
        SearchOptions::default(),
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("skip.log")),
        "paths: {paths:?}"
    );
    assert!(paths.iter().any(|p| p.ends_with("keep.txt")));
}

#[test]
fn empty_ignore_sources_indexes_gitignored_paths() {
    let tmp = TempDir::new().expect("tempdir");
    make_filter_corpus(tmp.path());

    let sift_dir = tmp.path().join(".sift");
    let tmp_path = tmp.path().to_path_buf();
    let root = tmp_path.canonicalize().unwrap_or(tmp_path);
    let meta = sample_store_meta_no_ignore(root, vec![IndexRecord::ngram(GramWidth::TRIGRAM)]);
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build");
    drop(indexes);

    let indexes = open_indexes(&sift_dir);
    let paths = candidate_paths(
        &indexes,
        tmp.path(),
        &["beta".to_string()],
        SearchOptions::default(),
    );
    assert!(
        paths.iter().any(|p| p.starts_with("skip")),
        "no-ignore build should index skip/: {paths:?}"
    );
}

#[test]
fn defaults_exclude_gitignored_and_ignore_file_paths() {
    let tmp = TempDir::new().expect("tempdir");
    make_filter_corpus(tmp.path());

    let sift_dir = tmp.path().join(".sift");
    build_indexes(tmp.path(), &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let paths = candidate_paths(
        &indexes,
        tmp.path(),
        &["beta".to_string()],
        SearchOptions::default(),
    );
    assert!(paths.iter().any(|p| p == Path::new("keep.txt")));
    assert!(paths.iter().any(|p| p == Path::new("root.txt")));
    assert!(!paths.iter().any(|p| p.starts_with("skip")));
    assert!(!paths.iter().any(|p| p.starts_with("also_skip")));
}

#[test]
fn build_respects_hidden_files_by_default() {
    let corpus = TempDir::new().expect("tempdir");
    fs::create_dir_all(corpus.path().join(".secret")).expect("create dir");
    fs::write(corpus.path().join(".secret/hidden.txt"), "beta\n").expect("write");

    let sift_dir = TempDir::new().expect("sift tempdir");
    let corpus_path = corpus.path().to_path_buf();
    let root = corpus_path.canonicalize().unwrap_or(corpus_path);
    let meta = sample_store_meta(root, vec![IndexRecord::ngram(GramWidth::TRIGRAM)]);
    let mut indexes = Indexes::open(sift_dir.path(), &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build");
    drop(indexes);

    let indexes = Indexes::open(sift_dir.path(), &meta).expect("open");
    let paths = candidate_paths(
        &indexes,
        corpus.path(),
        &["beta".to_string()],
        SearchOptions::default(),
    );
    assert!(
        !paths.iter().any(|p| p.starts_with(".secret")),
        "hidden files excluded by default: {paths:?}"
    );
}
