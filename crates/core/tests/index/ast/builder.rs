use std::fs;
use std::path::Path;

use sift_core::{AstIndex, AstLanguage, Files, GramWidth, IndexRecord, Indexes};
use tempfile::TempDir;

use crate::common::sample_store_meta;

fn write_polyglot_corpus(root: &Path) {
    fs::create_dir_all(root).expect("mkdir");
    fs::write(root.join("main.rs"), "fn main() { println!(\"hi\"); }\n").expect("write rs");
    fs::write(root.join("app.py"), "def run():\n    return 1\n").expect("write py");
    fs::write(root.join("index.js"), "function run() { return 1; }\n").expect("write js");
    fs::write(
        root.join("lib.ts"),
        "function run(): number { return 1; }\n",
    )
    .expect("write ts");
    fs::write(root.join("App.tsx"), "const x = <div>hi</div>;\n").expect("write tsx");
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc run() int { return 1 }\n",
    )
    .expect("write go");
    fs::write(
        root.join("Main.java"),
        "class Main { int run() { return 1; } }\n",
    )
    .expect("write java");
    fs::write(root.join("README.md"), "no language here\n").expect("write md");
    fs::write(root.join("invalid.rs"), [0xE2u8, 0x28, 0xA1, 0x0A]).expect("write invalid utf-8");
}

fn ast_catalog() -> Vec<IndexRecord> {
    vec![IndexRecord::ngram(GramWidth::TRIGRAM), IndexRecord::Ast]
}

#[test]
fn build_writes_ast_namespace_beside_ngram() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(root, ast_catalog());
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build");

    let id = indexes.current_id().expect("snapshot id");
    let snap = indexes.snapshot_dir(id);
    assert!(snap.join("files.bin").is_file());
    assert!(snap.join("ast").join("kinds.bin").is_file());
    assert!(snap.join("ast").join("postings.bin").is_file());
    assert!(!snap.join("ast").join("files.bin").exists());
    assert!(snap.join("ngram-3").join("postings.bin").is_file());

    // The store reopens through the normal search path.
    let reopened = Indexes::load(&sift_dir).expect("load").expect("some");
    assert!(reopened.queryable());
}

#[test]
fn build_and_reopen_maps_languages_by_extension() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);

    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(root, vec![IndexRecord::Ast]);
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot).expect("snapshot dir");
    let files = Files::build(&meta, &snapshot).expect("files");
    let ast_dir = tmp.path().join("ast");
    AstIndex::build(&ast_dir, &files).expect("build");

    let index = AstIndex::open(&ast_dir, files.len()).expect("open");
    assert_eq!(index.file_count(), files.len());
    assert!(index.stale_languages().is_empty());

    let of = |lang: AstLanguage| -> Vec<String> {
        index
            .files_of(lang)
            .into_iter()
            .filter_map(|id| files.rel_path(id).map(str::to_string))
            .collect()
    };
    assert_eq!(of(AstLanguage::Rust), vec!["main.rs".to_string()]);
    assert_eq!(of(AstLanguage::Python), vec!["app.py".to_string()]);
    assert_eq!(of(AstLanguage::JavaScript), vec!["index.js".to_string()]);
    // `.ts` and `.tsx` are distinct languages with distinct grammars.
    assert_eq!(of(AstLanguage::TypeScript), vec!["lib.ts".to_string()]);
    assert_eq!(of(AstLanguage::Tsx), vec!["App.tsx".to_string()]);
    assert_eq!(of(AstLanguage::Go), vec!["main.go".to_string()]);
    assert_eq!(of(AstLanguage::Java), vec!["Main.java".to_string()]);
}

#[test]
fn unsupported_and_non_utf8_files_have_no_language() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);

    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(root, vec![IndexRecord::Ast]);
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot).expect("snapshot dir");
    let files = Files::build(&meta, &snapshot).expect("files");
    let ast_dir = tmp.path().join("ast");
    AstIndex::build(&ast_dir, &files).expect("build");

    let index = AstIndex::open(&ast_dir, files.len()).expect("open");
    // invalid.rs is not valid UTF-8: present in the shared table, never a
    // structural candidate.
    let rust_files: Vec<String> = index
        .files_of(AstLanguage::Rust)
        .into_iter()
        .filter_map(|id| files.rel_path(id).map(str::to_string))
        .collect();
    assert!(!rust_files.contains(&"invalid.rs".to_string()));
    // README.md has no grammar; every corpus file is still covered by the
    // shared table even though only some have a language.
    let with_language: usize = AstLanguage::ALL
        .iter()
        .map(|&lang| index.files_of(lang).len())
        .sum();
    assert!(with_language < files.len());
}

#[test]
fn empty_corpus_builds_and_reopens() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");

    let sift_dir = tmp.path().join(".sift");
    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(root, ast_catalog());
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build");

    let id = indexes.current_id().expect("snapshot id").to_string();
    let snap = indexes.snapshot_dir(&id);
    assert!(snap.join("ast").join("kinds.bin").is_file());
    assert!(snap.join("ast").join("postings.bin").is_file());
    // Reopening the snapshot must not error even with zero files.
    drop(indexes);
    let reopened = Indexes::load(&sift_dir).expect("load").expect("some");
    assert_eq!(reopened.current_id(), Some(id.as_str()));
}

#[test]
fn ids_are_ascending_and_within_the_shared_space() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);

    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(root, vec![IndexRecord::Ast]);
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot).expect("snapshot dir");
    let files = Files::build(&meta, &snapshot).expect("files");
    let ast_dir = tmp.path().join("ast");
    AstIndex::build(&ast_dir, &files).expect("build");
    let index = AstIndex::open(&ast_dir, files.len()).expect("open");

    for &lang in &AstLanguage::ALL {
        let ids = index.files_of(lang);
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted, "{lang:?} ids must be ascending and unique");
        assert!(ids.iter().all(|id| id.get() < files.len()));
    }
}

#[test]
fn record_round_trips_name_and_serde() {
    assert_eq!(IndexRecord::Ast.name(), "ast");
    assert_eq!(IndexRecord::from_name("ast"), Ok(IndexRecord::Ast));
    let json = serde_json::to_string(&IndexRecord::Ast).expect("serialize");
    assert_eq!(json, "{\"kind\":\"ast\"}");
    let back: IndexRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, IndexRecord::Ast);
}
